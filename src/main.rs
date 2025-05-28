use rustsocks::net::AcceptOpts;
use rustsocks::redir_ext::{TcpListenerRedirExt, TcpStreamRedirExt};
use rustsocks::config::RedirType;
use std::{io::Result, net::SocketAddr};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
};

#[tokio::main]
async fn main() -> Result<()> {
    let arg_error = || {
        eprintln!("invalid arguments \nusage: rustsocks <listen address> <proxy address>");
        std::process::exit(1);
    };
    let mut args = std::env::args();
    let listen_addr_str = args.nth(1).unwrap_or_else(arg_error);
    let listen_addr = match listen_addr_str.parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(_) => {
            eprintln!(
                "invalid listen address \nexpected format: <ip>:<port> (e.g. 127.0.0.1:12345)"
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

    let listener =
        TcpListener::bind_redir(RedirType::PacketFilter, listen_addr, AcceptOpts::default())
            .await?;
    println!("rustsocks is listening on {}", listen_addr_str);

    loop {
        let (stream, _client_addr) = listener.accept().await?;
        #[cfg(debug_assertions)]
        println!("New client from: {}", _client_addr);
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, proxy_addr).await {
                eprintln!("Error: {}", e);
            }
        });
    }
}

async fn handle_client(mut client_stream: TcpStream, proxy: SocketAddr) -> Result<()> {
    let orig_dst = client_stream.destination_addr(RedirType::PacketFilter)?;
    #[cfg(debug_assertions)]
    println!("Original destination: {}", orig_dst);

    let mut proxy_stream = connect_http(proxy, orig_dst).await?;
    let _ = copy_bidirectional(&mut client_stream, &mut proxy_stream).await;
    Ok(())
}

async fn connect_http(proxy: SocketAddr, target: SocketAddr) -> Result<TcpStream> {
    // connect to http proxy
    let mut stream = TcpStream::connect(proxy).await?;

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
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "HTTP proxy error: {}",
                    resp
                ),
            ));
        }
    }

    Ok(stream)
}
