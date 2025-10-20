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
}

impl BasicSocket for Socks5UdpClient {
    async fn send_to<A>(&self, buf: &[u8], addr: A) -> io::Result<usize>
    where
        SocketAddr: From<A>,
    {
        let n = self.send_to(0, buf, addr).await?;
        Ok(n)
    }
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let (n, _, addr) = self.recv_from(buf).await?;
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
}
