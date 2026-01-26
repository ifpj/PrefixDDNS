mod config;
mod netlink;
mod web;
mod logging;

use config::ConfigManager;
use netlink::NetlinkMonitor;
use web::AppState;
use logging::log_to_web;
use tokio::sync::{broadcast, RwLock};
use std::sync::Arc;
use std::collections::VecDeque;
use colored::Colorize;
use chrono::Local;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Working directory
    #[arg(short = 'd', long)]
    work_dir: Option<PathBuf>,

    /// Config file path
    #[arg(short = 'c', long, default_value = "config.json")]
    config: String,

    /// Web server port
    #[arg(short = 'p', long, default_value_t = 3000)]
    port: u16,

    /// Network interface to monitor (e.g., eth0). If not specified, monitors all interfaces.
    #[arg(short = 'i', long)]
    interface: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Set working directory if specified
    if let Some(work_dir) = args.work_dir {
        std::env::set_current_dir(&work_dir)?;
    }

    // Initialize config
    let config_manager = ConfigManager::new(&args.config).await;

    // Create channels
    let (netlink_tx, mut netlink_rx) = broadcast::channel(16);
    let (log_tx, _) = broadcast::channel(100);

    // Shared state
    let state = AppState {
        config_manager: config_manager.clone(),
        log_tx: log_tx.clone(),
        recent_logs: Arc::new(RwLock::new(VecDeque::new())),
    };

    // Resolve interface index if interface name is provided
    let interface_index = if let Some(iface_name) = args.interface {
        match get_interface_index(&iface_name) {
            Ok(idx) => Some(idx),
            Err(e) => {
                let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string().dimmed();
                eprintln!("{} {} Error resolving interface '{}': {}", timestamp, "[Error]".red(), iface_name, e);
                return Err(e);
            }
        }
    } else {
        None
    };

    // Start Netlink Monitor
    let run_on_startup = config_manager.get_run_on_startup().await;
    let monitor = NetlinkMonitor::new(netlink_tx, run_on_startup, interface_index);
    tokio::spawn(async move {
        if let Err(e) = monitor.run().await {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string().dimmed();
            eprintln!("{} {} Netlink monitor error: {}", timestamp, "[Error]".red(), e);
        }
    });

    // Start Web Server
    let server_state = state.clone();
    let port = args.port;
    tokio::spawn(async move {
        web::start_server(server_state, port).await;
    });

    println!("{} {} PrefixDDNS started.", Local::now().format("%Y-%m-%d %H:%M:%S").to_string().dimmed(), "[Init]".green());

    // Initialize last_prefix based on current state and config
    let mut last_prefix: Option<u128> = None;
    let initial_ip = match NetlinkMonitor::get_current_ipv6(interface_index).await {
        Ok(Some(ip)) => {
            last_prefix = Some(get_prefix_64(ip));
            let msg = format!("Initial IP {} detected.", ip);
            // println!("{}", msg);
            let log_limit = config_manager.get_log_limit().await;
            log_to_web(&state.log_tx, &state.recent_logs, "System", "info", &msg, log_limit).await;
            Some(ip)
        }
        Err(e) => {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string().dimmed();
            eprintln!("{} {} Failed to fetch initial IP: {}", timestamp, "[Error]".red(), e);
            None
        }
        _ => None,
    };

    if run_on_startup {
        if let Some(ip) = initial_ip {
            let tasks = config_manager.get_tasks().await;
            let log_limit = config_manager.get_log_limit().await;
            
            let msg = format!("Startup execution: IPv6 prefix detected: {}", ip);
            log_to_web(&state.log_tx, &state.recent_logs, "Startup", "info", &msg, log_limit).await;
            
            process_tasks(&state, &tasks, ip, log_limit, "Startup").await;
        }
    }

    // Main loop: Process IPv6 changes
    while let Ok(ip) = netlink_rx.recv().await {
        let current_prefix = get_prefix_64(ip);

        if Some(current_prefix) == last_prefix {
            if std::env::var("DEBUG_DUPLICATE").is_ok() {
                // 截断后缀零，仅保留前64位前缀
                let prefix_trunc = current_prefix >> 64;
                let msg = format!("Duplicate IP: {}, Prefix: {:x}", ip, prefix_trunc);
                let log_limit = config_manager.get_log_limit().await;
                log_to_web(&state.log_tx, &state.recent_logs, "Netlink", "debug", &msg, log_limit).await;
            }
            continue;
        }
        last_prefix = Some(current_prefix);

        let tasks = config_manager.get_tasks().await;
        let log_limit = config_manager.get_log_limit().await;

        let msg = format!("New IPv6 prefix from: {}", ip);

        // Log detection
        log_to_web(&state.log_tx, &state.recent_logs, "Netlink", "info", &msg, log_limit).await;

        process_tasks(&state, &tasks, ip, log_limit, "Netlink").await;
    }

    Ok(())
}

fn get_interface_index(name: &str) -> anyhow::Result<u32> {
    let path = format!("/sys/class/net/{}/ifindex", name);
    let content = std::fs::read_to_string(&path)?;
    let index = content.trim().parse()?;
    Ok(index)
}

fn get_prefix_64(ip: std::net::Ipv6Addr) -> u128 {
    u128::from(ip) & 0xffff_ffff_ffff_ffff_0000_0000_0000_0000
}

async fn process_tasks(
    state: &AppState,
    tasks: &[config::Task],
    ip: std::net::Ipv6Addr,
    log_limit: usize,
    source: &str,
) {
    for task in tasks {
        if !task.enabled {
            continue;
        }
        match web::combine_ip(ip, &task.suffix) {
            Ok(combined) => {
                let log_msg = format!(
                    "Task [{}]: Running for {}",
                    task.name, combined
                );
                log_to_web(&state.log_tx, &state.recent_logs, source, "info", &log_msg, log_limit).await;

                match web::send_webhook(task, ip, combined, None).await {
                    Ok(status) => {
                        let success_msg = format!(
                            "Task [{}]: Success (HTTP {})",
                            task.name, status
                        );
                        log_to_web(&state.log_tx, &state.recent_logs, source, "success", &success_msg, log_limit).await;
                    }
                    Err(e) => {
                        let err_msg = format!(
                            "Task [{}]: Failed: {}",
                            task.name, e
                        );
                        log_to_web(&state.log_tx, &state.recent_logs, source, "error", &err_msg, log_limit).await;
                    }
                }
            }
            Err(e) => {
                let err_msg = format!(
                    "Task [{}]: IP combination failed: {}",
                    task.name, e
                );
                log_to_web(&state.log_tx, &state.recent_logs, source, "error", &err_msg, log_limit).await;
            }
        }
    }
}
