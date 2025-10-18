//! modified from shadowsocks-service/src/local/redir/sys/mod.rs

use socket2::Socket;
use std::io;

#[cfg(unix)]
#[allow(dead_code)]
pub fn set_ipv6_only<S>(socket: &S, ipv6_only: bool) -> io::Result<()>
where
    S: std::os::unix::io::AsRawFd,
{
    use std::os::unix::io::{FromRawFd, IntoRawFd};

    let fd = socket.as_raw_fd();
    let sock = unsafe { Socket::from_raw_fd(fd) };
    let result = sock.set_only_v6(ipv6_only);
    let _ = sock.into_raw_fd();
    result
}
