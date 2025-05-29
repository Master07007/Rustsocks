# Rustsocks

**A simple transparent TCP-to-proxy redirector, written in Rust.**

`rustsocks` is a lightweight tool similar to [redsocks](https://github.com/darkk/redsocks), designed for redirecting TCP traffic transparently to a HTTP proxy. It is especially useful when used together with firewall-based packet redirection (e.g. `pf` on macOS).

## Origin
This project is extracted and simplified from [`shadowsocks-service`](https://github.com/shadowsocks/shadowsocks-rust/tree/master/crates/shadowsocks-service), focusing specifically on the `pf`-related transparent proxy logic. All unrelated features have been removed to keep the binary minimal and purpose-specific.

## Features
- Transparent redirection of TCP connections
- Simple command-line usage
- High performance, low memory footprint

## Usage
**Notice: `rustsocks` needs root privilege to work!** This is because it needs access to system-level firewall mechanisms (such as `pf` on macOS) in order to query the original destination address of redirected connections. 
Without root access, `rustsocks` will not be able to determine where the incoming traffic was originally intended to go, and thus cannot properly forward it to the proxy.
```sh
sudo rustsocks <listen_address> <proxy_address>
```
- `listen_address`: Local address that `rustsocks` bind to (e.g. `127.0.0.1:12345`)
- `proxy_address`: HTTP proxy address (e.g. `127.0.0.1:20172`)

### Example
If you have configured `pf` to redirect traffic to `127.0.0.1:12345`, and are using `v2rayA` as your HTTP proxy listening on `127.0.0.1:20172`, you can simply run: 
```sh
sudo rustsocks 127.0.0.1:12345 127.0.0.1:20172
```

## Build
Requires Rust 1.70+.
```sh
git clone https://github.com/Master07007/Rustsocks.git
cd rustsocks
cargo build --release
```
The binary will be at `./target/release/rustsocks`.