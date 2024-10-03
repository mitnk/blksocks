use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SOCKS_VERSION: u8 = 5;
const CMD_CONNECT: u8 = 1;
const ADDR_TYPE_IPV4: u8 = 1;
const ADDR_TYPE_DOMAIN: u8 = 3;

pub async fn proxy_conn(
    proxy_addr: &str,
    dest_addr: &str,
) -> Result<TcpStream, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = match TcpStream::connect(proxy_addr).await {
        Ok(s) => s,
        Err(e) => return Err(format!("to socks5 server: {}", e).into()),
    };

    // Send SOCKS version and authentication methods
    stream.write_all(&[SOCKS_VERSION, 1, 0]).await?;
    let mut buf = [0; 2];
    stream.read_exact(&mut buf).await?;

    // Check for SOCKS version and authentication method
    if buf[0] != SOCKS_VERSION || buf[1] != 0 {
        return Err("Invalid SOCKS version or authentication method".into());
    }

    // Send SOCKS version, command, and dest address type
    let dest_addr_parts: Vec<&str> = dest_addr.split(':').collect();
    let dest_addr_str = dest_addr_parts[0];
    let dest_port = dest_addr_parts[1].parse::<u16>()?;
    let dest_port_bytes = dest_port.to_be_bytes();
    let addr_type = if dest_addr_str.parse::<std::net::Ipv4Addr>().is_ok() {
        ADDR_TYPE_IPV4
    } else {
        ADDR_TYPE_DOMAIN
    };

    let mut req = vec![SOCKS_VERSION, CMD_CONNECT, 0, addr_type];
    match addr_type {
        ADDR_TYPE_IPV4 => {
            req.extend_from_slice(&dest_addr_str.parse::<std::net::Ipv4Addr>()?.octets());
        }
        ADDR_TYPE_DOMAIN => {
            let addr_len = dest_addr_str.len() as u8;
            req.push(addr_len);
            req.extend_from_slice(dest_addr_str.as_bytes());
        }
        _ => return Err("Unsupported address type".into()),
    }

    req.extend_from_slice(&dest_port_bytes);
    stream.write_all(&req).await?;
    let mut buf = [0; 10];
    stream.read_exact(&mut buf).await?;

    Ok(stream)
}
