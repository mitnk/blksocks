use serde_derive::Deserialize;
use simple_logger::SimpleLogger;
use std::fs;
use std::path::Path;
use tokio::net::{TcpListener, TcpStream};

mod socks5;
mod utils;

#[derive(Debug, Deserialize)]
struct Config {
    listen: String,
    socks5: String,
}

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();

    let config = match read_config("/etc/blksocks/config.toml") {
        Ok(c) => c,
        Err(e) => {
            log::error!("config loading error: {}", e);
            return;
        }
    };

    let addr = config.listen;
    let listener = TcpListener::bind(&addr).await.unwrap();
    log::info!("Server started on {}", &addr);

    loop {
        let addr_socks5 = config.socks5.clone();
        let (socket, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            match handle_client(socket, &addr_socks5).await {
                Ok(_) => {}
                Err(e) => log::info!("{}", e),
            }
        });
    }
}

async fn handle_client(
    client_socket: TcpStream,
    addr_socks5: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let dest_addr = utils::get_dest_addr(&client_socket)?;
    log::info!("connecting to {}", &dest_addr);

    let downstream_socket = socks5::proxy_conn(addr_socks5, &dest_addr).await?;

    let (mut client_reader, mut client_writer) = client_socket.into_split();
    let (mut downstream_reader, mut downstream_writer) = downstream_socket.into_split();

    let client_to_server =
        tokio::spawn(
            async move { tokio::io::copy(&mut client_reader, &mut downstream_writer).await },
        );

    let server_to_client =
        tokio::spawn(
            async move { tokio::io::copy(&mut downstream_reader, &mut client_writer).await },
        );

    // Wait for either of the connections to finish transferring data
    let (res1, res2) = tokio::join!(client_to_server, server_to_client);

    if let Err(e) = res1 {
        log::error!("Client to server error: {:?}", e);
    }
    if let Err(e) = res2 {
        log::error!("Server to client error: {:?}", e);
    }

    Ok(())
}

fn read_config<P: AsRef<Path>>(path: P) -> Result<Config, Box<dyn std::error::Error>> {
    // Reading the file as a string
    let contents = fs::read_to_string(path)?;

    // Parsing the string into your configuration struct
    let config: Config = toml::from_str(&contents)?;

    Ok(config)
}
