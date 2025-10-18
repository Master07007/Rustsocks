use std::{io, net::SocketAddr};

use bytes::Bytes;
use lru_time_cache::LruCache;
use tokio::sync::mpsc;

use crate::udp_relay::{
    DEFAULT_UDP_EXPIRY_DURATION, UDP_ASSOCIATION_SEND_CHANNEL_SIZE, send::UdpSendWorker,
};

pub struct UdpNatManager {
    nat_map: LruCache<SocketAddr, UdpSendWorker>,
    keep_alive_sender: mpsc::Sender<SocketAddr>,
}

impl UdpNatManager {
    pub fn new() -> (Self, mpsc::Receiver<SocketAddr>) {
        let (keep_alive_sender, keep_alive_receiver) =
            mpsc::channel::<SocketAddr>(UDP_ASSOCIATION_SEND_CHANNEL_SIZE);
        (
            UdpNatManager {
                nat_map: LruCache::with_expiry_duration(DEFAULT_UDP_EXPIRY_DURATION),
                keep_alive_sender,
            },
            keep_alive_receiver,
        )
    }
    pub fn send_to(
        &mut self,
        peer_addr: SocketAddr,
        target: SocketAddr,
        data: Bytes,
    ) -> io::Result<()> {
        let worker = match self.nat_map.entry(peer_addr) {
            lru_time_cache::Entry::Occupied(w) => w.into_mut(),
            lru_time_cache::Entry::Vacant(e) => {
                log::debug!("created udp association for {}", peer_addr);
                let worker = UdpSendWorker::new(peer_addr, self.keep_alive_sender.clone())?;
                e.insert(worker)
            }
        };
        worker.send_to(target, data)
    }
    pub async fn cleanup_expired(&mut self) {
        self.nat_map.iter();
    }

    pub fn keep_alive(&mut self, peer_addr: &SocketAddr) {
        self.nat_map.get(peer_addr);
    }
}
