//! A tiny **local, unauthenticated** HTTP proxy that forwards to an upstream
//! residential proxy, injecting the upstream's credentials. Chrome can't
//! carry proxy `user:pass` in its flags, so each
//! profile points its browser at a loopback [`Forwarder`] instead, and this
//! shim speaks the authenticated handshake to the real 家宽 upstream.
//!
//! Only the browser needs this — the API clients (wreq/reqwest) embed
//! credentials in the proxy URL directly. Both still egress from the *same*
//! upstream residential IP, so login-IP ≡ operating-IP holds.
//!
//! The listener is no-auth but bound to `127.0.0.1` only. `CONNECT` (HTTPS, the
//! path that matters for logins) is fully handled for both HTTP and SOCKS5
//! upstreams; plain-HTTP requests are forwarded best-effort.

use std::io;

use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::{ProxyConfig, ProxyKind};

/// A running loopback forwarder. Aborts its accept loop when dropped.
pub struct Forwarder {
    port: u16,
    task: JoinHandle<()>,
}

impl Forwarder {
    /// The `127.0.0.1:<port>` a browser should be pointed at (as an HTTP proxy).
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Bind a loopback listener on a random free port and forward every connection
/// to `upstream` with credentials injected.
pub async fn spawn(upstream: ProxyConfig) -> io::Result<Forwarder> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((client, _)) = listener.accept().await else {
                break;
            };
            let upstream = upstream.clone();
            tokio::spawn(async move {
                if let Err(error) = handle(client, &upstream).await {
                    log::debug!("forwarded connection ended with an error: {error}");
                }
            });
        }
    });
    Ok(Forwarder { port, task })
}

/// Parsed first line of a client HTTP-proxy request.
struct Request {
    method: String,
    target: String,
    head: Vec<u8>,
    leftover: Vec<u8>,
}

async fn handle(mut client: TcpStream, upstream: &ProxyConfig) -> io::Result<()> {
    let req = read_request(&mut client).await?;
    if req.method.eq_ignore_ascii_case("CONNECT") {
        // target = host:port; establish a raw tunnel through the upstream.
        let mut up = connect_tunnel(upstream, &req.target).await?;
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        copy_bidirectional(&mut client, &mut up).await?;
        return Ok(());
    }
    // Plain HTTP (absolute-URI request line). Rare for our targets (all HTTPS),
    // handled best-effort so incidental http requests don't hard-fail.
    match upstream.kind {
        ProxyKind::Socks5 => {
            let (host, port, _path) = split_absolute_uri(&req.target);
            let mut up = socks5_connect(upstream, &host, port).await?;
            // Rewrite to origin-form so the target (not a proxy) understands it.
            let origin = rewrite_to_origin_form(&req.head, &req.target);
            up.write_all(&origin).await?;
            up.write_all(&req.leftover).await?;
            copy_bidirectional(&mut client, &mut up).await?;
        }
        ProxyKind::Http | ProxyKind::Https => {
            let mut up = TcpStream::connect((upstream.host.as_str(), upstream.port)).await?;
            let head = inject_proxy_auth(&req.head, upstream);
            up.write_all(&head).await?;
            up.write_all(&req.leftover).await?;
            copy_bidirectional(&mut client, &mut up).await?;
        }
    }
    Ok(())
}

/// Open an authenticated tunnel to `target` (`host:port`) through the upstream.
async fn connect_tunnel(upstream: &ProxyConfig, target: &str) -> io::Result<TcpStream> {
    match upstream.kind {
        ProxyKind::Socks5 => {
            let (host, port) = split_host_port(target);
            socks5_connect(upstream, &host, port).await
        }
        ProxyKind::Http | ProxyKind::Https => http_connect(upstream, target).await,
    }
}

/// HTTP `CONNECT` to an upstream HTTP proxy, with `Proxy-Authorization`.
async fn http_connect(upstream: &ProxyConfig, target: &str) -> io::Result<TcpStream> {
    let mut up = TcpStream::connect((upstream.host.as_str(), upstream.port)).await?;
    let mut head = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(auth) = basic_auth(upstream) {
        head.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
    }
    head.push_str("\r\n");
    up.write_all(head.as_bytes()).await?;

    let resp = read_head(&mut up).await?;
    let status_ok = resp
        .split(|&b| b == b'\n')
        .next()
        .map(|line| line.windows(3).any(|w| w == b"200"))
        .unwrap_or(false);
    if !status_ok {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "upstream refused CONNECT",
        ));
    }
    Ok(up)
}

/// SOCKS5 CONNECT to `host:port` through an upstream SOCKS5 proxy, with optional
/// username/password auth (RFC 1929).
async fn socks5_connect(upstream: &ProxyConfig, host: &str, port: u16) -> io::Result<TcpStream> {
    let mut up = TcpStream::connect((upstream.host.as_str(), upstream.port)).await?;
    let creds = upstream
        .username
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(|u| (u, upstream.password.as_deref().unwrap_or("")));

    // Greeting: offer no-auth (0x00) and/or user/pass (0x02).
    if creds.is_some() {
        up.write_all(&[0x05, 0x01, 0x02]).await?;
    } else {
        up.write_all(&[0x05, 0x01, 0x00]).await?;
    }
    let mut sel = [0u8; 2];
    up.read_exact(&mut sel).await?;
    if sel[0] != 0x05 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not socks5"));
    }
    match sel[1] {
        0x00 => {}
        0x02 => {
            let (user, pass) = creds.ok_or_else(|| {
                io::Error::new(io::ErrorKind::PermissionDenied, "upstream wants auth")
            })?;
            let mut msg = vec![0x01, user.len() as u8];
            msg.extend_from_slice(user.as_bytes());
            msg.push(pass.len() as u8);
            msg.extend_from_slice(pass.as_bytes());
            up.write_all(&msg).await?;
            let mut ok = [0u8; 2];
            up.read_exact(&mut ok).await?;
            if ok[1] != 0x00 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "socks5 auth failed",
                ));
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "no acceptable socks5 auth method",
            ));
        }
    }

    // CONNECT request, domain address type (0x03).
    let mut req = vec![0x05, 0x01, 0x00, 0x03, host.len() as u8];
    req.extend_from_slice(host.as_bytes());
    req.extend_from_slice(&port.to_be_bytes());
    up.write_all(&req).await?;

    let mut reply = [0u8; 4];
    up.read_exact(&mut reply).await?;
    if reply[1] != 0x00 {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "socks5 connect refused",
        ));
    }
    // Drain the bound address so the stream is positioned at the tunnel body.
    match reply[3] {
        0x01 => {
            let mut b = [0u8; 4 + 2];
            up.read_exact(&mut b).await?;
        }
        0x04 => {
            let mut b = [0u8; 16 + 2];
            up.read_exact(&mut b).await?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            up.read_exact(&mut len).await?;
            let mut b = vec![0u8; len[0] as usize + 2];
            up.read_exact(&mut b).await?;
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad socks5 atyp",
            ));
        }
    }
    Ok(up)
}

fn basic_auth(upstream: &ProxyConfig) -> Option<String> {
    let user = upstream.username.as_deref().filter(|u| !u.is_empty())?;
    let pass = upstream.password.as_deref().unwrap_or("");
    let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
    Some(format!("Basic {token}"))
}

/// Read an HTTP head (through the blank line) plus any bytes read past it.
async fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let (head, leftover) = read_head_split(stream).await?;
    let first = head.split(|&b| b == b'\n').next().unwrap_or(&[]).to_vec();
    let line = String::from_utf8_lossy(&first);
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    Ok(Request {
        method,
        target,
        head,
        leftover,
    })
}

async fn read_head(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    Ok(read_head_split(stream).await?.0)
}

/// Read until `\r\n\r\n`; return (head_including_terminator, extra_bytes_read).
async fn read_head_split(stream: &mut TcpStream) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
        if buf.len() > 64 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "head too large"));
        }
    }
    Ok((buf, Vec::new()))
}

fn inject_proxy_auth(head: &[u8], upstream: &ProxyConfig) -> Vec<u8> {
    let text = String::from_utf8_lossy(head);
    let mut out = String::new();
    let mut lines = text.split("\r\n");
    if let Some(first) = lines.next() {
        out.push_str(first);
        out.push_str("\r\n");
    }
    if let Some(auth) = basic_auth(upstream) {
        out.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
    }
    for line in lines {
        if line
            .split(':')
            .next()
            .map(|n| n.trim().eq_ignore_ascii_case("proxy-authorization"))
            .unwrap_or(false)
        {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// Rewrite `GET http://host/path` → `GET /path` for a raw (SOCKS) tunnel.
fn rewrite_to_origin_form(head: &[u8], absolute: &str) -> Vec<u8> {
    let (_, _, path) = split_absolute_uri(absolute);
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or("");
    let method = first.split_whitespace().next().unwrap_or("GET");
    let mut out = format!("{method} {path} HTTP/1.1\r\n");
    for line in lines {
        let lower = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if lower == "proxy-authorization" || lower == "proxy-connection" {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.into_bytes()
}

fn split_host_port(target: &str) -> (String, u16) {
    match target.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(443)),
        None => (target.to_string(), 443),
    }
}

/// `http://host:port/path` → (host, port, "/path").
fn split_absolute_uri(uri: &str) -> (String, u16, String) {
    let rest = uri
        .strip_prefix("http://")
        .or_else(|| uri.strip_prefix("https://"))
        .unwrap_or(uri);
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(80)),
        None => (authority.to_string(), 80),
    };
    (host, port, path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(kind: ProxyKind, port: u16) -> ProxyConfig {
        ProxyConfig {
            kind,
            host: "127.0.0.1".into(),
            port,
            username: Some("foo".into()),
            password: Some("bar".into()),
        }
    }

    /// Echo origin: echoes back whatever it receives (one connection).
    async fn spawn_echo() -> u16 {
        let l = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            loop {
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                if s.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });
        port
    }

    /// Mock upstream HTTP proxy requiring `Proxy-Authorization: Basic Zm9vOmJhcg==`
    /// (foo:bar). Honors CONNECT by tunnelling to the requested target.
    async fn spawn_http_upstream() -> u16 {
        let l = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let accepted = l.accept().await;
                let Ok((mut c, _)) = accepted else {
                    break;
                };
                tokio::spawn(async move {
                    let head = read_head(&mut c).await.unwrap();
                    let text = String::from_utf8_lossy(&head);
                    let authed = text.contains("Proxy-Authorization: Basic Zm9vOmJhcg==");
                    let target = text.split_whitespace().nth(1).unwrap_or("").to_string();
                    if !authed {
                        // The 407 is this fixture's terminal reply; the client
                        // under test may already have closed.
                        if let Err(error) = c
                            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                            .await
                        {
                            eprintln!("mock proxy 407 write failed: {error}");
                        }
                        return;
                    }
                    let mut up = TcpStream::connect(target).await.unwrap();
                    c.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                        .await
                        .unwrap();
                    // Either side closing ends the tunnel; the test asserts the
                    // client-observed outcome.
                    if let Err(error) = copy_bidirectional(&mut c, &mut up).await {
                        eprintln!("mock tunnel ended with an error: {error}");
                    }
                });
            }
        });
        port
    }

    /// Mock upstream SOCKS5 proxy requiring user/pass foo/bar.
    async fn spawn_socks5_upstream() -> u16 {
        let l = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let accepted = l.accept().await;
                let Ok((mut c, _)) = accepted else {
                    break;
                };
                tokio::spawn(async move {
                    let mut g = [0u8; 2];
                    c.read_exact(&mut g).await.unwrap();
                    let mut methods = vec![0u8; g[1] as usize];
                    c.read_exact(&mut methods).await.unwrap();
                    c.write_all(&[0x05, 0x02]).await.unwrap(); // demand user/pass
                    let mut h = [0u8; 2];
                    c.read_exact(&mut h).await.unwrap();
                    let mut user = vec![0u8; h[1] as usize];
                    c.read_exact(&mut user).await.unwrap();
                    let mut pl = [0u8; 1];
                    c.read_exact(&mut pl).await.unwrap();
                    let mut pass = vec![0u8; pl[0] as usize];
                    c.read_exact(&mut pass).await.unwrap();
                    let ok = user == b"foo" && pass == b"bar";
                    c.write_all(&[0x01, if ok { 0x00 } else { 0x01 }])
                        .await
                        .unwrap();
                    if !ok {
                        return;
                    }
                    let mut req = [0u8; 4];
                    c.read_exact(&mut req).await.unwrap();
                    let host = match req[3] {
                        0x03 => {
                            let mut len = [0u8; 1];
                            c.read_exact(&mut len).await.unwrap();
                            let mut h = vec![0u8; len[0] as usize];
                            c.read_exact(&mut h).await.unwrap();
                            String::from_utf8(h).unwrap()
                        }
                        0x01 => {
                            let mut b = [0u8; 4];
                            c.read_exact(&mut b).await.unwrap();
                            format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
                        }
                        _ => return,
                    };
                    let mut pb = [0u8; 2];
                    c.read_exact(&mut pb).await.unwrap();
                    let tport = u16::from_be_bytes(pb);
                    let mut up = TcpStream::connect((host.as_str(), tport)).await.unwrap();
                    c.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                        .await
                        .unwrap();
                    // Either side closing ends the tunnel; the test asserts the
                    // client-observed outcome.
                    if let Err(error) = copy_bidirectional(&mut c, &mut up).await {
                        eprintln!("mock tunnel ended with an error: {error}");
                    }
                });
            }
        });
        port
    }

    async fn tunnel_and_echo(fwd_port: u16, echo_port: u16) {
        let mut c = TcpStream::connect(("127.0.0.1", fwd_port)).await.unwrap();
        c.write_all(
            format!(
                "CONNECT 127.0.0.1:{echo_port} HTTP/1.1\r\nHost: 127.0.0.1:{echo_port}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        let head = read_head(&mut c).await.unwrap();
        assert!(
            String::from_utf8_lossy(&head).contains("200"),
            "expected 200, got: {}",
            String::from_utf8_lossy(&head)
        );
        c.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        c.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
    }

    #[tokio::test]
    async fn http_upstream_connect_injects_basic_auth() {
        let echo = spawn_echo().await;
        let up = spawn_http_upstream().await;
        let fwd = spawn(cfg(ProxyKind::Http, up)).await.unwrap();
        tunnel_and_echo(fwd.port(), echo).await;
    }

    #[tokio::test]
    async fn socks5_upstream_connect_injects_userpass() {
        let echo = spawn_echo().await;
        let up = spawn_socks5_upstream().await;
        let fwd = spawn(cfg(ProxyKind::Socks5, up)).await.unwrap();
        tunnel_and_echo(fwd.port(), echo).await;
    }
}
