use std::{io, net::SocketAddr};
use tokio::net::UdpSocket;

use crate::utils::socks::udp_client::Socks5UdpClient;

pub mod socks5;
pub mod tcp_client;
pub mod udp_client;

pub trait BasicSocket: Sized + Send + 'static {
    fn send_to<A>(&self, buf: &[u8], addr: A) -> impl Future<Output = io::Result<usize>> + Send
    where
        SocketAddr: From<A>,
        A: Send;
    fn recv_from(
        &self,
        buf: &mut [u8],
    ) -> impl Future<Output = io::Result<(usize, SocketAddr)>> + Send;
    fn bind(bind_addr: SocketAddr) -> impl Future<Output = io::Result<Self>> + Send;
}

impl BasicSocket for UdpSocket {
    async fn send_to<A>(&self, buf: &[u8], addr: A) -> io::Result<usize>
    where
        SocketAddr: From<A>,
    {
        let addr: SocketAddr = addr.into();
        self.send_to(buf, addr).await
    }
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.recv_from(buf).await
    }
    async fn bind(bind_addr: SocketAddr) -> io::Result<Self> {
        UdpSocket::bind(bind_addr).await
    }
}

impl BasicSocket for Socks5UdpClient {
    async fn send_to<A>(&self, buf: &[u8], addr: A) -> io::Result<usize>
    where
        SocketAddr: From<A>,
    {
        self.send_to(0, buf, addr)
            .await
            .map_err(|e| io::Error::other(e))
    }
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let (n, _, addr) = self.recv_from(buf).await.map_err(|e| io::Error::other(e))?;
        let addr = match addr {
            socks5::Address::SocketAddress(addr) => addr,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid address type",
                ));
            }
        };
        Ok((n, addr))
    }
    async fn bind(bind_addr: SocketAddr) -> io::Result<Self> {
        let mut socket = Socks5UdpClient::bind(bind_addr).await?;
        socket
            .associate("127.0.0.1:20170")
            .await
            .map_err(|e| io::Error::other(e))?;
        Ok(socket)
    }
}
