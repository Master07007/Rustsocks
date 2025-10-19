use crate::{
    udp_relay::{MAXIMUM_UDP_PAYLOAD_SIZE, UDP_ASSOCIATION_SEND_CHANNEL_SIZE, checker::Checker},
    utils::{raw_socket::RawSocket, socks::BasicSocket},
};
use bytes::Bytes;
use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, Receiver},
    task::JoinHandle,
};

pub struct Direct;
pub struct Proxy(SocketAddr);

trait BindAddr<S: BasicSocket> {
    fn bind(&self, bind_addr: SocketAddr) -> impl Future<Output = io::Result<S>>;
}

impl BindAddr<UdpSocket> for Direct {
    async fn bind(&self, bind_addr: SocketAddr) -> io::Result<UdpSocket> {
        UdpSocket::bind(bind_addr).await
    }
}

pub struct UdpSendWorker {
    sender: mpsc::Sender<(SocketAddr, Bytes)>,
    worker_handle: JoinHandle<()>,
}

impl UdpSendWorker {
    pub fn new<S: BasicSocket>(
        peer_addr: SocketAddr,
        keep_alive_sender: mpsc::Sender<SocketAddr>,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel(UDP_ASSOCIATION_SEND_CHANNEL_SIZE);
        let mut dispatcher: Dispatcher<S> = Dispatcher::new(peer_addr, keep_alive_sender)?;
        let worker_handle = tokio::spawn(async move {
            dispatcher.dispatch_packet(receiver).await;
        });
        Ok(Self {
            sender,
            worker_handle,
        })
    }
    pub fn send_to(&self, target: SocketAddr, data: Bytes) -> io::Result<()> {
        self.sender
            .try_send((target, data))
            .map_err(|e| io::Error::other(e))
    }

    pub fn worker_handle(&self) -> &JoinHandle<()> {
        &self.worker_handle
    }
}

// the servers and clients are N:N, we may need more sockets
struct Dispatcher<S: BasicSocket> {
    peer_addr: SocketAddr,
    client_to_server: Option<S>,
    server_to_client: RawSocket,
    keep_alive_sender: mpsc::Sender<SocketAddr>,
    buffer: Box<[u8]>,
}

impl<S: BasicSocket> Dispatcher<S> {
    fn new(peer_addr: SocketAddr, keep_alive_sender: mpsc::Sender<SocketAddr>) -> io::Result<Self> {
        let buffer = vec![0u8; MAXIMUM_UDP_PAYLOAD_SIZE].into_boxed_slice();
        let server_to_client =
            RawSocket::new().inspect_err(|_| log::error!("Can not create raw socket!"))?;
        Ok(Self {
            peer_addr,
            client_to_server: None,
            server_to_client,
            keep_alive_sender,
            buffer,
        })
    }

    async fn dispatch_packet(&mut self, mut receiver: Receiver<(SocketAddr, Bytes)>) {
        let mut checker = Checker::new(Duration::from_secs(1));
        loop {
            tokio::select! {
                // 1. receive and send packets to server
                //    also update the last active time
                receive_opt = receiver.recv() => {
                    let (target_addr, data) = match receive_opt{
                        Some(d) => d,
                        None => {
                            log::trace!("udp association for {} -> ... channel closed", self.peer_addr);
                            break;
                        }
                    };
                    self.handle_client_packets(target_addr, data).await;
                }
                // 2. receive packets from server
                receive_result = self.receive_server_packets() => {
                    let (n, remote_addr) = match receive_result {
                        Ok(s) => s,
                        Err(err) => {
                            log::error!("udp relay {} <- ... failed, error: {}", self.peer_addr, err);
                            // Socket failure. Reset for recreation.
                            self.client_to_server = None;
                            continue;
                        }
                    };
                    checker.activate();
                    self.handle_server_packets(remote_addr, n).await;
                }
                // 3. keep-alive check
                _ = checker.wait() => {
                    log::trace!("send keep alive msg");
                    if self.keep_alive_sender.try_send(self.peer_addr).is_err() {
                        log::debug!("udp relay {} keep-alive failed, channel full or closed", self.peer_addr);
                        checker.activate();
                    }
                }
            }
        }
    }

    async fn handle_client_packets(&mut self, target_addr: SocketAddr, data: Bytes) {
        log::trace!(
            "udp relay {} -> {} with {} bytes",
            self.peer_addr,
            target_addr,
            data.len()
        );
        if let Err(e) = self.send_client_packets(target_addr, &data).await {
            log::error!(
                "udp relay {} -> {} with {} bytes, error: {}",
                self.peer_addr,
                target_addr,
                data.len(),
                e
            );
        }
    }

    async fn send_client_packets(
        &mut self,
        target_addr: SocketAddr,
        data: &Bytes,
    ) -> io::Result<()> {
        let socket = match &mut self.client_to_server {
            Some(socket) => socket,
            None => {
                // create a new socket
                let bind_addr: SocketAddr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);
                let socket = S::bind(bind_addr).await?;
                self.client_to_server.insert(socket)
            }
        };
        let n = socket.send_to(&data, target_addr).await?;
        if n != data.len() {
            log::warn!(
                "{} -> {} sent {} bytes != expected {} bytes",
                self.peer_addr,
                target_addr,
                n,
                data.len()
            );
        }
        Ok(())
    }

    async fn receive_server_packets(&mut self) -> io::Result<(usize, SocketAddr)> {
        match &mut self.client_to_server {
            Some(socket) => socket.recv_from(&mut self.buffer).await,
            None => futures::future::pending().await,
        }
    }

    async fn handle_server_packets(&mut self, target_addr: SocketAddr, recv_len: usize) {
        log::trace!(
            "udp relay {} <- {} with {} bytes",
            self.peer_addr,
            target_addr,
            recv_len
        );
        if let Err(e) = self.send_server_packets(target_addr, recv_len).await {
            log::error!(
                "udp relay {} <- {} with {} bytes, error: {}",
                self.peer_addr,
                target_addr,
                recv_len,
                e
            );
        }
    }

    async fn send_server_packets(
        &mut self,
        remote_addr: SocketAddr,
        recv_len: usize,
    ) -> io::Result<()> {
        let n = self
            .server_to_client
            .send_to(remote_addr, self.peer_addr, &self.buffer[..recv_len])
            .await?;
        if n != recv_len {
            log::warn!(
                "udp relay {} <- {} with {} bytes != expected {} bytes",
                self.peer_addr,
                remote_addr,
                n,
                recv_len
            );
        }
        Ok(())
    }
}
