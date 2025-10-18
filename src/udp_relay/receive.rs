use crate::{
    udp_relay::{DEFAULT_UDP_EXPIRY_DURATION, macos::UdpRedirSocket},
    utils::expiry_map::ExpiryMap,
};
use std::{
    io,
    net::SocketAddr,
    sync::{Arc, LazyLock},
};

pub struct UdpReceiveManager;

struct Context {
    nat_map: ExpiryMap<SocketAddr, Arc<UdpRedirSocket>>,
}

static CONTEXT: LazyLock<Context> = LazyLock::new(UdpReceiveManager::create);

impl UdpReceiveManager {
    fn create() -> Context {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(DEFAULT_UDP_EXPIRY_DURATION).await;
                CONTEXT.nat_map.cleanup_expired();
            }
        });
        let context = Context {
            nat_map: ExpiryMap::new(DEFAULT_UDP_EXPIRY_DURATION),
        };
        context
    }

    pub async fn send_to(
        peer_addr: SocketAddr,
        remote_addr: SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        let socket = if let Some(socket) = CONTEXT.nat_map.get_mut(&remote_addr) {
            // clone the socket here to avoid awaiting with a lock(very dangerous, may cause deadlock)
            socket.clone()
        } else {
            let socket = UdpRedirSocket::bind_nonlocal(
                crate::utils::config::RedirType::PacketFilter,
                remote_addr,
            )?;
            let socket = Arc::new(socket);
            CONTEXT.nat_map.insert(remote_addr, socket.clone());
            socket
        };
        let n = socket.send_to(data, peer_addr).await?;
        if n < data.len() {
            log::warn!(
                "udp redir send back data (actual: {} bytes, sent: {} bytes), remote: {}, peer: {}",
                n,
                data.len(),
                remote_addr,
                peer_addr
            );
        }

        log::trace!(
            "udp redir send back data {} bytes, remote: {}, peer: {}",
            n,
            remote_addr,
            peer_addr,
        );
        Ok(())
    }

    pub async fn cleanup_expired() {
        CONTEXT.nat_map.cleanup_expired();
    }
}
