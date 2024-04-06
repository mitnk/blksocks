use nix::sys::socket::{getsockopt, sockopt::OriginalDst};
use std::io;
use std::net::SocketAddrV4;
use tokio::net::TcpStream;

pub fn get_dest_addr(client_socket: &TcpStream) -> io::Result<String> {
    let addr = getsockopt(&client_socket, OriginalDst)?;
    let addr_v4 = SocketAddrV4::new(
        u32::from_be(addr.sin_addr.s_addr).into(),
        u16::from_be(addr.sin_port),
    );

    Ok(format!("{}", addr_v4))
}

pub fn _print_data(data: &[u8]) {
    match std::str::from_utf8(data) {
        Ok(display_str) => {
            println!("{}", display_str);
        }
        Err(_) => {
            let clean_string = data
                .iter()
                .map(|&c| {
                    if c.is_ascii_graphic() || c == b' ' {
                        c
                    } else {
                        b'.'
                    }
                })
                .collect::<Vec<u8>>();
            println!("{}", std::str::from_utf8(&clean_string).unwrap());
        }
    }
}
