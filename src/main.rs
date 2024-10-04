use nix::libc;
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use std::{env, fs};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

mod logging;
mod socks5;
mod stats;
mod utils;

type SharedStats = Arc<Mutex<HashMap<IpAddr, stats::PacketStats>>>;

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

#[derive(Debug, Default, Deserialize)]
struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    enabled: bool,
    #[serde(default = "default_file_size_limit_mb")]
    file_size_limit_mb: u64,
    #[serde(default = "default_rotate_count")]
    rotate_count: usize,
}

fn default_logging_enabled() -> bool {
    true
}
fn default_file_size_limit_mb() -> u64 {
    2
}
fn default_rotate_count() -> usize {
    5
}

fn main() {
    if let Err(e) = daemonize() {
        eprintln!("daemonizing error: {}", e);
        exit(1);
    }

    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("tokio runtime error: {}", e);
            exit(1);
        }
    };
    runtime.block_on(async {
        blk_main().await;
    });
}

async fn blk_main() {
    let config = match read_config("/etc/blksocks/config.toml") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config loading error: {}", e);
            return;
        }
    };

    let packet_stats = Arc::new(Mutex::new(HashMap::<IpAddr, stats::PacketStats>::new()));
    tokio::spawn(expire_old_entries(Arc::clone(&packet_stats)));
    tokio::spawn(handle_user1(Arc::clone(&packet_stats)));

    let addr = config.network.listen;
    let listener = match TcpListener::bind(&addr).await {
        Ok(x) => x,
        Err(e) => {
            eprintln!("bind error: {}", e);
            exit(1);
        }
    };

    logging::setup(&config.logging);

    // do not close fds until end of all possible error reports
    null_fd(0);
    null_fd(1);
    null_fd(2);

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
            let mut pstats = packet_stats_clone.lock().await;
            stats::update_stats(&mut pstats, ip, bytes_copied);
        }
        Result::<_, Box<dyn std::error::Error + Send + Sync>>::Ok(())
    });

    let server_to_client = tokio::spawn(async move {
        let bytes_copied = tokio::io::copy(&mut downstream_reader, &mut client_writer).await?;
        if let Ok(addr) = dest_addr.parse::<SocketAddr>() {
            let ip = addr.ip();
            let mut pstats = packet_stats.lock().await;
            stats::update_stats(&mut pstats, ip, bytes_copied);
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

async fn expire_old_entries(shared_stats: SharedStats) {
    let mut interval = interval(Duration::from_secs(86_400)); // 24 hours
    loop {
        interval.tick().await;
        let mut pstats = shared_stats.lock().await;
        stats::expire_old_entries(&mut pstats);
    }
}

async fn handle_user1(shared_stats: SharedStats) {
    let mut sigusr1 = match signal(SignalKind::user_defined1()) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("handle usr1 error: {}", e);
            return;
        }
    };

    while sigusr1.recv().await.is_some() {
        let pstats = shared_stats.lock().await;
        let tops = stats::get_top_ips(&pstats);
        log::info!("Top IPs by byte count:");
        for (ip, bytes) in tops {
            log::info!("- {}: {} bytes", ip, bytes);
        }
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

    unsafe {
        libc::umask(0);
    }
    env::set_current_dir("/")?;

    Ok(())
}

fn null_fd(fd: i32) {
    let dev_null = OpenOptions::new().read(true).write(true).open("/dev/null");
    match dev_null {
        Ok(dev_null) => {
            let fd_null = dev_null.as_raw_fd();
            unsafe {
                libc::dup2(fd_null, fd);
                libc::close(fd_null);
            }
        }
        Err(e) => {
            eprintln!("get dev_null fd error: {}", e);
        }
    }
}
