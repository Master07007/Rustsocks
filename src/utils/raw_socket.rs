use core::slice;
use libc::{in_addr, size_t, sockaddr, sockaddr_in, socklen_t};
use std::{
    alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error},
    ffi::c_void,
    io, mem,
    net::SocketAddr,
    os::fd::RawFd,
    ptr::copy_nonoverlapping,
};
use tokio::io::unix::AsyncFd;

struct Packet {
    data: *mut u8,
    layout: Layout,
}

impl Packet {
    fn new(buf: &[u8]) -> Self {
        unsafe {
            let header_len = mem::size_of::<IpHeader>() + mem::size_of::<UdpHeader>();
            let layout = Layout::from_size_align_unchecked(header_len + buf.len(), 4);
            let data = alloc_zeroed(layout);
            if data.is_null() {
                handle_alloc_error(layout);
            }
            copy_nonoverlapping(buf.as_ptr(), data.add(header_len), buf.len());
            Packet { data, layout }
        }
    }
    fn as_headers(&mut self) -> (&mut IpHeader, &mut UdpHeader) {
        let ip_header = unsafe { &mut *(self.data as *mut IpHeader) };
        let udp_header =
            unsafe { &mut *(self.data.add(mem::size_of::<IpHeader>()) as *mut UdpHeader) };
        (ip_header, udp_header)
    }

    fn len(&self) -> usize {
        self.layout.size()
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.data, self.layout);
        }
    }
}

#[repr(C)]
struct IpHeader {
    ver_ihl: u8,
    /// reserved field, never use
    tos: u8,
    tot_len: u16,
    id: u16,
    frag_off: u16,
    ttl: u8,
    protocol: u8,
    check: u16,
    saddr: u32,
    daddr: u32,
}

#[repr(C)]
struct UdpHeader {
    sport: u16,
    dport: u16,
    len: u16,
    check: u16,
}

const UDP_HEADER_LEN: usize = mem::size_of::<UdpHeader>();
const IP_HEADER_LEN: usize = mem::size_of::<IpHeader>();
const HEADER_LEN: usize = UDP_HEADER_LEN + IP_HEADER_LEN;

struct SendParam {
    len: size_t,
    addr: sockaddr_in,
    addrlen: socklen_t,
    packet: Packet,
}

unsafe impl Send for SendParam {}

impl UdpHeader {
    fn as_slice_mut(&mut self) -> &mut [u8] {
        // SAFETY: there is always only one mutable reference
        unsafe { slice::from_raw_parts_mut(self as *mut _ as *mut u8, mem::size_of::<UdpHeader>()) }
    }
}

impl IpHeader {
    fn as_slice_mut(&mut self) -> &mut [u8] {
        // SAFETY: there is always only one mutable reference
        unsafe { slice::from_raw_parts_mut(self as *mut _ as *mut u8, mem::size_of::<IpHeader>()) }
    }
}

pub struct RawSocket {
    fd: AsyncFd<RawFd>,
}

impl RawSocket {
    pub fn new() -> std::io::Result<Self> {
        let fd = unsafe {
            let fd = libc::socket(libc::AF_INET, libc::SOCK_RAW, libc::IPPROTO_UDP);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let one: libc::c_int = 1;
            if libc::setsockopt(
                fd,
                libc::IPPROTO_IP,
                libc::IP_HDRINCL,
                &raw const one as *const c_void,
                mem::size_of::<libc::c_int>() as socklen_t,
            ) < 0
            {
                libc::close(fd);
                return Err(std::io::Error::last_os_error());
            }
            // these two operations will never fail
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            fd
        };
        let async_fd = AsyncFd::new(fd)?;
        Ok(RawSocket { fd: async_fd })
    }

    /// send raw UDP packet
    pub async fn send_to(
        &self,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        buf: &[u8],
    ) -> io::Result<usize> {
        if buf.len() > 1500 - 28 {
            log::warn!(
                "UDP packet len is longger than 1500, but fragment is not supported for now"
            );
        }
        let param = Self::build_packet(src_addr, dst_addr, buf);
        loop {
            let mut guard = self.fd.writable().await?;
            let n = unsafe {
                libc::sendto(
                    *guard.get_inner(),
                    param.packet.data as *const c_void,
                    param.len,
                    0,
                    &param.addr as *const sockaddr_in as *const sockaddr,
                    param.addrlen,
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    guard.clear_ready();
                    continue;
                } else {
                    return Err(err);
                }
            } else {
                return Ok((n as usize).saturating_sub(HEADER_LEN));
            }
        }
    }

    /// build raw UDP packet with IP and UDP headers
    fn build_packet(src_addr: SocketAddr, dst_addr: SocketAddr, buf: &[u8]) -> SendParam {
        let (src_ip, dst_ip) = match (src_addr.ip(), dst_addr.ip()) {
            (std::net::IpAddr::V4(a), std::net::IpAddr::V4(b)) => (a, b),
            _ => unimplemented!("not support IPv6 yet!"),
        };
        let src_port = src_addr.port();
        let dst_port = dst_addr.port();
        let mut packet = Packet::new(buf);
        let (ip_header, udp_header) = packet.as_headers();
        // build UDP header
        // UDP header length is 8 bytes
        udp_header.sport = src_port.to_be();
        udp_header.dport = dst_port.to_be();
        udp_header.len = ((mem::size_of::<UdpHeader>() + buf.len()) as u16).to_be();
        // For simplicity, we set checksum to 0 (no checksum)
        udp_header.check = 0;
        // build IP header
        // version=4, IHL=5 (20 bytes)
        ip_header.ver_ihl = (4 << 4) | 5;
        // macOS and freebsd(before 11.0) use host byte order for ip_len and ip_off
        ip_header.tot_len =
            (mem::size_of::<UdpHeader>() + mem::size_of::<IpHeader>() + buf.len()) as u16;
        // 0 means kernel set appropriate value
        ip_header.id = 0;
        // no fragmentation
        ip_header.frag_off = 0;
        ip_header.ttl = 64;
        ip_header.protocol = libc::IPPROTO_UDP as u8;
        // not computing IP checksum for simplicity
        ip_header.check = 0;
        ip_header.saddr = u32::from_be_bytes(src_ip.octets()).to_be();
        ip_header.daddr = u32::from_be_bytes(dst_ip.octets()).to_be();

        // build dst sockaddr_in
        let dst = sockaddr_in {
            sin_len: mem::size_of::<sockaddr_in>() as u8,
            sin_family: libc::AF_INET as u8,
            // not strictly used for raw
            sin_port: 0,
            // not used by kernel, but we still need to fill it, 0 will cause `NotConnected` error
            sin_addr: in_addr {
                s_addr: u32::from_be_bytes(dst_ip.octets()).to_be(),
            },
            sin_zero: [0i8; 8],
        };
        SendParam {
            len: packet.len() as size_t,
            addr: dst,
            addrlen: mem::size_of::<sockaddr_in>() as socklen_t,
            packet: packet,
        }
    }
}

#[test]
fn test_ub() {
    let buf = [3u8; 10];
    let mut packet = Packet::new(&buf);
    let (ip_header, udp_header) = packet.as_headers();
    let slice = ip_header.as_slice_mut();
    slice[1] = 10;
    println!("ip header slice: {:?}", ip_header.as_slice_mut());
    ip_header.tot_len = 3800;
    udp_header.len = 18;
    println!("packet slice: {:?}", unsafe {
        slice::from_raw_parts(packet.data, packet.layout.size())
    })
}

#[test]
fn test_ub_2() {
    use std::net::Ipv4Addr;
    let src_addr = SocketAddr::new(Ipv4Addr::new(1, 2, 2, 2).into(), 52345);
    let dst_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 52346);
    let buf = b"hello raw socket";
    let param = RawSocket::build_packet(src_addr, dst_addr, buf);
    let buf = param.packet.data;
    let buf_slice = unsafe { slice::from_raw_parts(buf as *const u8, param.len) };
    println!("built packet: {:?}", buf_slice);
}

#[ignore = "this test needs root privilege"]
#[tokio::test]
async fn test_sendto() {
    use std::net::Ipv4Addr;
    use tokio::net::UdpSocket;
    let raw_socket = RawSocket::new().unwrap();
    let src_addr = SocketAddr::new(Ipv4Addr::new(1, 2, 2, 2).into(), 52345);
    let dst_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 52346);
    let socket = UdpSocket::bind(dst_addr).await.unwrap();
    let handle = tokio::spawn(async move {
        let mut buf = [0u8; 16];
        let (n, addr) = socket.recv_from(&mut buf).await.unwrap();
        println!("received {} bytes from {}", n, addr);
        println!("data: {:?}", String::from_utf8_lossy(&buf[..n]));
        (buf, n)
    });
    let buf = b"hello raw socket";
    let n = raw_socket.send_to(src_addr, dst_addr, buf).await.unwrap();
    println!("sent {} bytes", n);
    let (recv_buf, recv_n) = handle.await.unwrap();
    assert_eq!(*buf, recv_buf);
    assert_eq!(n, recv_n);
}
