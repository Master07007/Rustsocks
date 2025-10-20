# Rustsocks

**A simple transparent TCP/UDP-to-proxy redirector, written in Rust.**

`rustsocks` is a lightweight tool similar to [redsocks](https://github.com/darkk/redsocks), designed for redirecting TCP/UDP traffic transparently to a HTTP/SOCKS5 proxy. It is especially useful when used together with firewall-based packet redirection (e.g. `pf` on macOS).

## Origin
This project is extracted and simplified from [shadowsocks-rust](https://github.com/shadowsocks/shadowsocks-rust), focusing specifically on the `pf`-related transparent proxy logic. All unrelated features have been removed to keep the binary minimal and purpose-specific.

## Features
- Transparent redirection of TCP/UDP connections
- Simple command-line usage
- High performance, low memory footprint

## Todo
- [ ] Support IPv6
- [ ] Support config file

## Usage
**Notice: `rustsocks` needs root privilege to work!** This is because it needs access to system-level firewall mechanisms (such as `pf` on macOS) in order to query the original destination address of redirected connections.
Without root access, `rustsocks` will not be able to determine where the incoming traffic was originally intended to go, and thus cannot properly forward it to the proxy.
```sh
rustsocks <listen address(forward to proxy)> <listen address(direct)> <proxy address> <socks5 proxy address(optional)>
```
- `listen_address(forward to proxy)`: The local address that `rustsocks` bind to (e.g. `127.0.0.1:12345`). All TCP/UDP packets received on this address will be forwarded through the proxy.
- `listen address(direct)`: The local address that `rustsocks` bind to (e.g. `127.0.0.1:12346`). All TCP/UDP packets received on this address will be forwarded directly without using a proxy.
- `proxy_address`: The HTTP proxy address (e.g. `127.0.0.1:20172`)
- `socks5 proxy address` (*optional*): The SOCKS5 proxy address (e.g. `127.0.0.1:20170`). If omitted, TCP and UDP packets received on the direct listen address will still be forwarded directly, but UDP packets received on the proxy listen address will be ignored.

### Example
If you have configured `pf` to redirect traffic to `127.0.0.1:12345`,
and want `rustsocks` to forward that traffic through an HTTP proxy running on `127.0.0.1:20172`,
while also allowing direct connections via `127.0.0.1:12346`,
and an optional SOCKS5 proxy on `127.0.0.1:20170`, you can run:
```sh
rustsocks 127.0.0.1:12345 127.0.0.1:12346 127.0.0.1:20172 127.0.0.1:20170
```

## Build
Requires the latest stable Rust toolchain. Older versions may not compile successfully.
```sh
git clone https://github.com/Master07007/Rustsocks.git
cd rustsocks
cargo build --release
```
The binary will be at `./target/release/rustsocks`.
