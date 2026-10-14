//! Manual probe check: `cargo run -p onecrew-proxy --example probe [proxy_url]`
//! With no arg it probes direct (this machine's egress IP).

#[tokio::main]
async fn main() {
    let proxy = std::env::args().nth(1);
    match onecrew_proxy::check_egress_ip(proxy.as_deref()).await {
        Ok(info) => println!(
            "egress: {info}  (ip={}, country={}, org={})",
            info.ip, info.country, info.org
        ),
        Err(error) => eprintln!("failed: {error}"),
    }
}
