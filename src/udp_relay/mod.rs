use crate::{
    redir::redir_ext::UdpSocketRedirExt,
    udp_relay::{macos::UdpRedirSocket, manager::UdpNatManager, send::BindAddr},
    utils::socks::BasicSocket,
};
use bytes::Bytes;
use std::{io, net::SocketAddr, time::Duration};
use tokio::time;

pub mod checker;
pub mod macos;
pub mod manager;
pub mod receive;
pub mod send;

/// Default UDP association's expire duration
const DEFAULT_UDP_EXPIRY_DURATION: Duration = Duration::from_secs(5 * 60);
/// The maximum UDP payload size
const MAXIMUM_UDP_PAYLOAD_SIZE: usize = 65536;
/// Default association expire time
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Packet size for all UDP associations' send queue
pub const UDP_ASSOCIATION_SEND_CHANNEL_SIZE: usize = 1024;

pub async fn run<S, T>(listener: UdpRedirSocket, proxy_type: T)
where
    S: BasicSocket,
    T: BindAddr<S>,
{
    let mut pkt_buf = vec![0u8; MAXIMUM_UDP_PAYLOAD_SIZE].into_boxed_slice();
    // NOTE: use default expiry duration, it may be not the best
    let mut cleanup_timer = time::interval(DEFAULT_UDP_EXPIRY_DURATION);
    let (mut manager, mut keepalive_rx) = UdpNatManager::new(proxy_type);
    loop {
        tokio::select! {
            _ = cleanup_timer.tick() => {
                // cleanup expired associations. iter() will remove expired elements
                manager.cleanup_expired().await;
            }
            peer_addr_opt = keepalive_rx.recv() => {
                let peer_addr = peer_addr_opt.expect("keep-alive channel closed unexpectly");
                manager.keep_alive(&peer_addr);
            }

            // receive the redirected udp packet
            recv_result = listener.recv_dest_from(&mut pkt_buf) => {
                // though we can do zero copy, reuse the buffer seems more efficient
                handle_recv_result(recv_result, &pkt_buf, &mut manager).await;
            }
        }
    }
}

async fn handle_recv_result<S, T>(
    recv_result: io::Result<(usize, SocketAddr, SocketAddr)>,
    pkt_buf: &[u8],
    manager: &mut UdpNatManager<T, S>,
) where
    S: BasicSocket,
    T: BindAddr<S>,
{
    log::trace!("recv_dest_from:");
    let (recv_len, peer, dst) = match recv_result {
        Ok(o) => o,
        Err(err) => {
            log::error!("recv_dest_from failed with err: {}", err);
            return;
        }
    };
    log::trace!("recv_dest_from success");

    // Packet length is limited by MAXIMUM_UDP_PAYLOAD_SIZE, excess bytes will be discarded.
    // Copy the slice, it may be very small
    let pkt = Bytes::copy_from_slice(&pkt_buf[..recv_len]);

    log::trace!(
        "received UDP packet from {}, destination {}, length {} bytes",
        peer,
        dst,
        recv_len
    );

    if recv_len == 0 {
        // For windows, it will generate a ICMP Port Unreachable Message
        // https://docs.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-recvfrom
        // Which will result in recv_from return 0.
        //
        // It cannot be solved here, because `WSAGetLastError` is already set.
        //
        // See `relay::udprelay::utils::create_socket` for more detail.
        return;
    }

    // Try to convert IPv4 mapped IPv6 address for dual-stack mode.
    // if let SocketAddr::V6(a) = &dst
    //     && let Some(v4) = a.ip().to_ipv4_mapped()
    // {
    //     dst = SocketAddr::new(IpAddr::from(v4), a.port());
    // }

    if let Err(err) = manager.send_to(peer, dst, pkt) {
        log::debug!(
            "udp packet relay {} -> {} with {} bytes failed, error: {}",
            peer,
            dst,
            recv_len,
            err
        );
    }
}
