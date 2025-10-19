use libc::{RLIMIT_NOFILE, getrlimit, rlimit};
use rustsocks::redir::redir_ext::{TcpListenerRedirExt, TcpStreamRedirExt};
use rustsocks::udp_relay::macos::UdpRedirSocket;
use rustsocks::udp_relay::run;
use rustsocks::utils::config::RedirType;
use rustsocks::utils::net::AcceptOpts;
use std::{io::Result, net::SocketAddr};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::join;
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
};

#[tokio::main]
async fn main() -> Result<()> {
    // check fd limit
    let mut limit = rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let res = unsafe { getrlimit(RLIMIT_NOFILE, &mut limit) };
    if res != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if limit.rlim_cur < 4096 {
        eprintln!(
            "Error: current file descriptor limit is {}, which is too low for the program to run reliably.",
            limit.rlim_cur
        );
        eprintln!(
            "Hint: increase the limit to at least 4096.\n\
             On macOS, you can temporarily raise it with:\n\
             \tlaunchctl limit maxfiles 4096\n\
             Note: changes may require restarting the current terminal session or logging out and back in to take effect.\n\
             To make it permanent across restarts, you may need to create a launchd service."
        );
        std::process::exit(1);
    }

    // check if running as root
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("You should run rustsocks with root privilege!");
        std::process::exit(1);
    }
    let arg_error = || {
        eprintln!(
            "invalid arguments \nusage: rustsocks <listen address(forward to proxy)> <listen address(direct)> <proxy address>"
        );
        std::process::exit(1);
    };
    let mut args = std::env::args();
    let listen_addr_proxy = args.nth(1).unwrap_or_else(arg_error);
    let listen_addr_proxy = match listen_addr_proxy.parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(_) => {
            eprintln!(
                "invalid listen address(forward to proxy) \nexpected format: <ip>:<port> (e.g. 127.0.0.1:12345)"
            );
            std::process::exit(1);
        }
    };
    let listen_addr_direct = args.next().unwrap_or_else(arg_error);
    let listen_addr_direct = match listen_addr_direct.parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(_) => {
            eprintln!(
                "invalid listen address(direct) \nexpected format: <ip>:<port> (e.g. 127.0.0.1:12345)"
            );
            std::process::exit(1);
        }
    };
    let proxy_addr = match args.next().unwrap_or_else(arg_error).parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(_) => {
            eprintln!(
                "invalid proxy address \nexpected format: <ip>:<port> (e.g. 127.0.0.1:20172)"
            );
            std::process::exit(1);
        }
    };

    let mut accept_options = AcceptOpts::default();
    accept_options.tcp.fastopen = true;
    accept_options.tcp.nodelay = true;
    accept_options.tcp.mptcp = true;

    let listener_proxy = TcpListener::bind_redir(
        RedirType::PacketFilter,
        listen_addr_proxy,
        AcceptOpts::default(),
    )
    .await
    .inspect_err(|_| {
        eprintln!("bind listen address(forward to proxy) error:");
    })?;
    let listener_direct = TcpListener::bind_redir(
        RedirType::PacketFilter,
        listen_addr_direct,
        AcceptOpts::default(),
    )
    .await
    .inspect_err(|_| {
        eprintln!("bind listen address(direct) error:");
    })?;
    env_logger::init();
    log::info!(
        "rustsocks is listening on \nProxy: {}\nDirect: {}",
        listen_addr_proxy,
        listen_addr_direct
    );
    let udp_socket = UdpRedirSocket::listen(RedirType::PacketFilter, listen_addr_proxy)?;

    let results = join!(
        accept_stream_proxy(&listener_proxy, proxy_addr),
        accept_stream_direct(&listener_direct),
        run(udp_socket)
    );
    results.0?;
    results.1?;
    Ok(())
}

async fn accept_stream_direct(listener: &TcpListener) -> Result<()> {
    loop {
        let (stream, _client_addr) = listener.accept().await?;
        #[cfg(debug_assertions)]
        log::info!("Direct: New client from: {}", _client_addr);
        tokio::spawn(async move {
            if let Err(e) = handle_client_direct(stream).await {
                log::error!("Error: {}", e);
            }
        });
    }
}

async fn accept_stream_proxy(listener: &TcpListener, proxy_addr: SocketAddr) -> Result<()> {
    loop {
        let (stream, _client_addr) = listener.accept().await?;
        #[cfg(debug_assertions)]
        log::info!("Proxy: New client from: {}", _client_addr);
        tokio::spawn(async move {
            if let Err(e) = handle_client_with_proxy(stream, proxy_addr).await {
                log::error!("Error: {}", e);
            }
        });
    }
}

async fn handle_client_with_proxy(mut client_stream: TcpStream, proxy: SocketAddr) -> Result<()> {
    let orig_dst = client_stream
        .destination_addr(RedirType::PacketFilter)
        .inspect_err(|e| log::error!("proxy stream: get original addr error: {e}"))?;
    #[cfg(debug_assertions)]
    log::info!("Original destination: {}", orig_dst);

    let mut proxy_stream = connect_http(proxy, orig_dst).await?;
    let _ = copy_bidirectional(&mut client_stream, &mut proxy_stream).await;
    // .inspect_err(|e| log::error!("proxy stream: copy error: {e}"))?;
    Ok(())
}

async fn handle_client_direct(mut client_stream: TcpStream) -> Result<()> {
    let orig_dst = client_stream
        .destination_addr(RedirType::PacketFilter)
        .inspect_err(|e| log::error!("direct stream: get original addr error: {e}"))?;

    #[cfg(debug_assertions)]
    log::info!("Original destination: {}", orig_dst);

    let mut another_stream = TcpStream::connect(orig_dst)
        .await
        .inspect_err(|e| log::error!("connect direct error: {e}"))?;
    let _ = copy_bidirectional(&mut client_stream, &mut another_stream).await;
    // .inspect_err(|e| log::error!("direct stream: copy error: {e}"))?;
    Ok(())
}

async fn connect_http(proxy: SocketAddr, target: SocketAddr) -> Result<TcpStream> {
    // connect to http proxy
    let mut stream = TcpStream::connect(proxy)
        .await
        .inspect_err(|e| log::error!("connect proxy error: {e}"))?;

    let connect_req = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
        target.ip(),
        target.port(),
        target.ip(),
        target.port()
    );

    stream.write_all(connect_req.as_bytes()).await?;

    // parse response
    let mut buf = [0u8; 1024];
    #[cfg(not(debug_assertions))]
    let _ = stream.read(&mut buf).await?;
    #[cfg(debug_assertions)]
    {
        let n = stream.read(&mut buf).await?;
        let resp = String::from_utf8_lossy(&buf[..n]);
        if !resp.starts_with("HTTP/1.1 200") {
            return Err(std::io::Error::other(format!("HTTP proxy error: {}", resp)));
        }
    }

    Ok(stream)
}
