use nix::libc;
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::{env, fs};
use std::fs::OpenOptions;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

mod logging;
mod socks5;
mod stats;
mod utils;

#[derive(Debug, Deserialize)]
struct Config {
    network: NetworkConfig,
    #[serde(default)]
    logging: LoggingConfig,
}

#[derive(Debug, Deserialize)]
struct NetworkConfig {
    listen: String,
    socks5: String,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    enabled: bool,
    #[serde(default = "default_file_size_limit_mb")]
    file_size_limit_mb: u64,
    #[serde(default = "default_rotate_count")]
    rotate_count: usize,
}

fn default_logging_enabled() -> bool { true }
fn default_file_size_limit_mb() -> u64 { 2 }
fn default_rotate_count() -> usize { 5 }

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            enabled: default_logging_enabled(),
            file_size_limit_mb: default_file_size_limit_mb(),
            rotate_count: default_rotate_count(),
        }
    }
}

fn main() {
    if let Err(e) = daemonize() {
        eprintln!("Error daemonizing: {}", e);
        exit(1);
    }

    let runtime = Runtime::new().unwrap();
    runtime.block_on(async { blk_func().await; });
}

async fn blk_func() {
    let config = match read_config("/etc/blksocks/config.toml") {
        Ok(c) => c,
        Err(e) => {
            println!("config loading error: {}", e);
            return;
        }
    };

    logging::setup(&config.logging);

    let packet_stats = Arc::new(Mutex::new(HashMap::<IpAddr, stats::PacketStats>::new()));
    let signal_stats = Arc::clone(&packet_stats);

    tokio::spawn(async move {
        let mut sigusr1 = signal(SignalKind::user_defined1()).expect("Failed to listen for SIGUSR1");
        while sigusr1.recv().await.is_some() {
            let stats = signal_stats.lock().await;
            let tops = stats::get_top_ips(&stats);
            log::info!("Top IPs by byte count:");
            for (ip, bytes) in tops {
                log::info!("- {}: {} bytes", ip, bytes);
            }
        }
    });

    tokio::spawn(expire_old_entries_periodically(Arc::clone(&packet_stats)));

    let addr = config.network.listen;
    let listener = TcpListener::bind(&addr).await.unwrap();
    log::info!("Server started on {}", &addr);
    log::info!("using proxy: {}", &config.network.socks5);

    loop {
        let addr_socks5 = config.network.socks5.clone();
        let packet_stats = Arc::clone(&packet_stats);

        let (socket, _) = match listener.accept().await {
            Ok((sock, _addr)) => (sock, _addr),
            Err(e) => {
                log::error!("accept failed: {}", e);
                break;
            }
        };

        tokio::spawn(async move {
            let result = handle_client(socket, &addr_socks5, packet_stats).await;
            if let Err(e) = result {
                log::info!("{}", e);
            }
        });
    }
}

async fn handle_client(
    client_socket: TcpStream,
    addr_socks5: &str,
    packet_stats: Arc<Mutex<HashMap<IpAddr, stats::PacketStats>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dest_addr = utils::get_dest_addr(&client_socket)?;
    log::info!("connecting to {}", &dest_addr);

    let downstream_socket = socks5::proxy_conn(addr_socks5, &dest_addr).await?;

    let (mut client_reader, mut client_writer) = client_socket.into_split();
    let (mut downstream_reader, mut downstream_writer) = downstream_socket.into_split();

    let dest_addr_clone = dest_addr.clone();
    let packet_stats_clone = Arc::clone(&packet_stats);
    let client_to_server = tokio::spawn(async move {
        let bytes_copied = tokio::io::copy(&mut client_reader, &mut downstream_writer).await?;
        if let Ok(addr) = dest_addr_clone.parse::<SocketAddr>() {
            let ip = addr.ip();
            let mut stats = packet_stats_clone.lock().await;
            stats::update_stats(&mut stats, ip, bytes_copied);
        }
        Result::<_, Box<dyn std::error::Error + Send + Sync>>::Ok(())
    });

    let server_to_client = tokio::spawn(async move {
        let bytes_copied = tokio::io::copy(&mut downstream_reader, &mut client_writer).await?;
        if let Ok(addr) = dest_addr.parse::<SocketAddr>() {
            let ip = addr.ip();
            let mut stats = packet_stats.lock().await;
            stats::update_stats(&mut stats, ip, bytes_copied);
        }
        Result::<_, Box<dyn std::error::Error + Send + Sync>>::Ok(())
    });

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
    let contents = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&contents)?;

    Ok(config)
}

async fn expire_old_entries_periodically(
    packet_stats: Arc<Mutex<HashMap<IpAddr, stats::PacketStats>>>,
) {
    let mut interval = interval(Duration::from_secs(86_400)); // 24 hours
    loop {
        interval.tick().await;
        let mut stats = packet_stats.lock().await;
        stats::expire_old_entries(&mut stats);
    }
}

fn daemonize() -> Result<(), std::io::Error> {
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if pid > 0 {
        // Parent exit
        exit(0);
    }

    let sid = unsafe { libc::setsid() };
    if sid < 0 {
        return Err(std::io::Error::last_os_error());
    }

    unsafe {
        libc::signal(libc::SIGCHLD, libc::SIG_IGN);
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }

    // Fork the second time
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if pid > 0 {
        exit(0);
    }

    unsafe { libc::umask(0); }
    env::set_current_dir("/")?;

    let dev_null = OpenOptions::new().read(true).write(true).open("/dev/null")?;
    let fd = dev_null.as_raw_fd();
    unsafe {
        libc::dup2(fd, 0);
        libc::dup2(fd, 1);
        libc::dup2(fd, 2);
        libc::close(fd);
    }

    Ok(())
}
