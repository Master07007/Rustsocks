//! Options for connecting to remote server
use std::{
    io,
    mem,
    net::SocketAddr,
    os::unix::io::AsRawFd,
    time::Duration,
};
use log::error;

/// Options for connecting to TCP remote server
#[derive(Debug, Clone, Default)]
pub struct TcpSocketOpts {
    /// TCP socket's `SO_SNDBUF`
    pub send_buffer_size: Option<u32>,

    /// TCP socket's `SO_RCVBUF`
    pub recv_buffer_size: Option<u32>,

    /// `TCP_NODELAY`
    pub nodelay: bool,

    /// `TCP_FASTOPEN`, enables TFO
    pub fastopen: bool,

    /// `SO_KEEPALIVE` and sets `TCP_KEEPIDLE`, `TCP_KEEPINTVL` and `TCP_KEEPCNT` respectively,
    /// enables keep-alive messages on connection-oriented sockets
    pub keepalive: Option<Duration>,

    /// Enable Multipath-TCP (mptcp)
    /// https://en.wikipedia.org/wiki/Multipath_TCP
    ///
    /// Currently only supported on
    /// - macOS (iOS, watchOS, ...) with Client Support only.
    /// - Linux (>5.19)
    pub mptcp: bool,
}

/// Options for UDP server
#[derive(Debug, Clone, Default)]
pub struct UdpSocketOpts {
    /// Maximum Transmission Unit (MTU) for UDP socket `recv`
    ///
    /// NOTE: MTU includes IP header, UDP header, UDP payload
    pub mtu: Option<usize>,

    /// Outbound UDP socket allows IP fragmentation
    pub allow_fragmentation: bool,
}

/// Inbound connection options
#[derive(Clone, Debug, Default)]
pub struct AcceptOpts {
    /// TCP options
    pub tcp: TcpSocketOpts,

    /// UDP options
    pub udp: UdpSocketOpts,

    /// Enable IPV6_V6ONLY option for socket
    pub ipv6_only: bool,
}

/// Check if `SocketAddr` could be used for creating dual-stack sockets
pub fn is_dual_stack_addr(addr: &SocketAddr) -> bool {
    if let SocketAddr::V6(ref v6) = *addr {
        let ip = v6.ip();
        ip.is_unspecified() || ip.to_ipv4_mapped().is_some()
    } else {
        false
    }
}

/// Enable `TCP_FASTOPEN`
///
/// `TCP_FASTOPEN` was supported since
/// macosx(10.11), ios(9.0), tvos(9.0), watchos(2.0)
pub fn set_tcp_fastopen<S: AsRawFd>(socket: &S) -> io::Result<()> {
    let enable: libc::c_int = 1;

    unsafe {
        let ret = libc::setsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_TCP,
            libc::TCP_FASTOPEN,
            &enable as *const _ as *const libc::c_void,
            mem::size_of_val(&enable) as libc::socklen_t,
        );

        if ret != 0 {
            let err = io::Error::last_os_error();
            error!("set TCP_FASTOPEN error: {}", err);
            return Err(err);
        }
    }

    Ok(())
}