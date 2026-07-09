use std::fs;
use std::env::args;
use std::sync::Arc;
use std::io::{self, Write};
use std::collections::BTreeMap;
use std::process::{exit, Command};

use automations::config;
use automations::logger;

use serde::Deserialize;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
struct AutomationsConfig {
    username: String,
    password: String,
    hosts: BTreeMap<String, Host>
}

#[derive(Debug, Deserialize)]
struct Host {
    _ip: String,
    vms: BTreeMap<String, String>
}

#[derive(Debug)]
struct Args {
    automation_name: String,
    host_owner_name: String,
    selected_hosts: Vec<u16>,
    selected_bots: Vec<u16>,
    additional_data_path: Option<String>
}

impl Args {
    fn new() -> Self {
        Self {
            automation_name: String::new(),
            host_owner_name: String::new(),
            selected_hosts: Vec::new(),
            selected_bots: Vec::new(),
            additional_data_path: None,
        }
    }
}

enum ArgKind {
    AutomationName,
    HostOwnerName,
    SelectedHosts,
    SelectedBots,
    AdditionalDataPath
}

#[tokio::main]
async fn main() {
    let app_config = config::Config::build();
    let _otel_guard = logger::init(&app_config);
    let automations_config: Arc<AutomationsConfig> = Arc::new(
        toml::from_str(&fs::read_to_string("config.toml").expect("Failed to read config.toml")).expect("Failed to parse TOML")
    );
    let args: Arc<Args> = Arc::new(parse_args());
    tracing::info!("args: {:?}", args);
    let update_needed = check_git_for_updates();
    let remote_command = Arc::new(get_remote_command(args.automation_name.as_str()));
    let sshpass_args: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut sshpass_args_guard = sshpass_args.lock().await;
    sshpass_args_guard.push(format!("-p{}", automations_config.password));
    sshpass_args_guard.push(String::from("ssh"));
    drop(sshpass_args_guard);
    run_automation(automations_config, args, sshpass_args, remote_command, update_needed).await;
}

fn parse_args() -> Args {
    let args_map: BTreeMap<&str, ArgKind> = BTreeMap::from([
        ("-a", ArgKind::AutomationName),
        ("--automation", ArgKind::AutomationName),
        ("-o", ArgKind::HostOwnerName),
        ("--owner", ArgKind::HostOwnerName),
        ("-h", ArgKind::SelectedHosts),
        ("--hosts", ArgKind::SelectedHosts),
        ("-b", ArgKind::SelectedBots),
        ("--bots", ArgKind::SelectedBots),
        ("-d", ArgKind::AdditionalDataPath),
        ("--data", ArgKind::AdditionalDataPath),
    ]);
    let args: Vec<String> = args().collect();
    let args_len = args.len();

    if args_len < 2 {
        tracing::error!("Usage: {} <automation_name>", args[0]);
        exit(1);
    }

    let mut args_struct: Args = Args::new();
    for i in (1..args_len).step_by(2) {
        if let Some(arg) = args_map.get(args[i].as_str()) {
            match arg {
                ArgKind::AutomationName => {
                    args_struct.automation_name = args[i + 1].clone();
                }
                ArgKind::HostOwnerName => {
                    args_struct.host_owner_name = args[i + 1].clone();
                }
                ArgKind::SelectedHosts => {
                    args_struct.selected_hosts = parse_range(args[i + 1].as_str());
                }
                ArgKind::SelectedBots => {
                    args_struct.selected_bots = parse_range(args[i + 1].as_str());
                }
                ArgKind::AdditionalDataPath => {
                    args_struct.additional_data_path = Some(args[i + 1].clone());
                }
            }
        }
        else {
            tracing::error!("Invalid argument: {}", args[i]);
            exit(1);
        }
    }
    args_struct
}

fn parse_range(range: &str) -> Vec<u16> {
    let range_parts = range.split('-').collect::<Vec<&str>>();
    if range_parts.len() == 1 {
        return vec![range_parts[0].parse::<u16>().expect("Failed to parse range")];
    }
    else {
        let start = range_parts[0].parse::<u16>().expect("Failed to parse range");
        let end = range_parts[1].parse::<u16>().expect("Failed to parse range");
        (start..=end).collect()
    }
}

fn check_git_for_updates() -> bool{
    let local_commit_hash_output = Command::new("git")
    .args(&["rev-parse", "HEAD"])
    .output()
    .expect("Failed to execute: git rev-parse HEAD");
    assert!(local_commit_hash_output.status.success(), "Failed to execute: git rev-parse HEAD: {}", &local_commit_hash_output.status);
    let local_commit_hash = String::from_utf8(local_commit_hash_output.stdout).expect("Failed to convert stdout to string")
                            .split_whitespace()
                            .next().expect("Failed to get local commit hash")
                            .to_string();
    tracing::info!("local_commit_hash: {}", local_commit_hash);

    let remote_commit_hash_output = Command::new("git")
    .args(&["ls-remote", "origin", "-h", "refs/heads/main"])
    .output()
    .expect("Failed to execute: git ls-remote origin -h refs/heads/main");
    assert!(remote_commit_hash_output.status.success(), "Failed to execute: git ls-remote origin -h refs/heads/main: {}", &remote_commit_hash_output.status);
    let remote_commit_hash = String::from_utf8(remote_commit_hash_output.stdout).expect("Failed to convert stdout to string")
                            .split_whitespace()
                            .next().expect("Failed to get remote commit hash")
                            .to_string();
    tracing::info!("remote_commit_hash: {}", remote_commit_hash);

    local_commit_hash != remote_commit_hash
}

fn get_remote_command(command: &str) -> String {
    match command {
        "launch_dota" => String::from("schtasks /run /tn \"start-dota\""),
        "shutdown_dota" => String::from("\"C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations\\dota_shutdown.bat\""),
        "cancel_game_search" => String::from("cd C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations && \"venv/Scripts/activate.bat\" && python cancel_game_search.py"),
        _ => {
            tracing::error!(%command, "The remote command not found");
            exit(1);
        },
    }
}

async fn run_automation(
    automations_config: Arc<AutomationsConfig>, 
    args: Arc<Args>, 
    sshpass_args: Arc<Mutex<Vec<String>>>, 
    remote_command: Arc<String>, 
    update_needed: bool
) {
    let (hosts_start_range, hosts_end_range) = (args.selected_hosts.first(), args.selected_hosts.last());
    let (bots_start_range, bots_end_range) = (args.selected_bots.first(), args.selected_bots.last());

    for (name, host) in automations_config.hosts.iter() {
        if !args.host_owner_name.is_empty() && !name.contains(&args.host_owner_name) {
            tracing::info!(%name, "Skipping host");
            continue;
        }

        if let (Some(hosts_start_range), Some(hosts_end_range)) = (hosts_start_range, hosts_end_range) {
            let host_number = parse_host_number(name);
            if host_number < *hosts_start_range || host_number > *hosts_end_range {
                tracing::info!(%name, "Skipping host (not in selected range)");
                continue;
            }
        }
        
        if let (Some(bots_start_range), Some(bots_end_range)) = (bots_start_range, bots_end_range) {
            tracing::info!(%name, "Running host");
            run_automation_on_bots(
                Arc::clone(&automations_config), 
                Arc::clone(&sshpass_args), 
                Arc::clone(&remote_command), 
                host,
                update_needed,
                *bots_start_range, 
                *bots_end_range
            ).await;
        }
        else {
            tracing::info!(%name, "Running host");
            run_automation_on_bots(
                Arc::clone(&automations_config), 
                Arc::clone(&sshpass_args), 
                Arc::clone(&remote_command), 
                host, 
                update_needed,
                1, 
                host.vms.len() as u16
            ).await;
        }
    }
}

fn parse_host_number(host_name: &str) -> u16 {
    host_name
        .rsplit("_host")
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("Failed to parse host number from: {}", host_name))
}

async fn run_automation_on_bots(
    automations_config: Arc<AutomationsConfig>, 
    sshpass_args: Arc<Mutex<Vec<String>>>, 
    remote_command: Arc<String>, 
    host: &Host, 
    update_needed: bool,
    bots_start_range: u16, 
    bots_end_range: u16
) {
    tracing::info!("Running automation on bots from range: {} to {}", &bots_start_range, &bots_end_range);
    let mut bot_number = 1;
    let mut ssh_sessions = Vec::new();
    for (name, ip) in host.vms.iter() {
        tracing::info!(%bot_number, "Checking bot number");
        if !(bot_number >= bots_start_range && bot_number <= bots_end_range) {
            tracing::info!(%name, "Skipping bot (not in selected range)");
            continue;
        }
        else {
            let automations_config = Arc::clone(&automations_config);
            let sshpass_args = Arc::clone(&sshpass_args);
            let remote_command = Arc::clone(&remote_command);
            let ip = ip.clone();
            let name = name.clone();
            let ssh_session = tokio::spawn(async move {
                execute_ssh_command(automations_config, sshpass_args, remote_command, update_needed, &name, &ip).await;
            });
            ssh_sessions.push(ssh_session);
        }
        bot_number += 1;
    }
    for ssh_session in ssh_sessions {
        ssh_session.await.unwrap();
    }
}

async fn execute_ssh_command(
    automations_config: Arc<AutomationsConfig>, 
    sshpass_args: Arc<Mutex<Vec<String>>>, 
    remote_command: Arc<String>, 
    update_needed: bool, 
    name: &str,
    ip: &str
) {
    let mut sshpass_args_guard = sshpass_args.lock().await;
    if let Some(_arg) = sshpass_args_guard.get(2) {
        sshpass_args_guard[2] = format!("{}@{}", automations_config.username, ip);
        sshpass_args_guard[3] = get_final_command(update_needed, &*remote_command);
    }
    else {
        sshpass_args_guard.push(format!("{}@{}", automations_config.username, ip));
        sshpass_args_guard.push(get_final_command(update_needed, &*remote_command));
    }

    let output = Command::new("sshpass")
    .args(&*sshpass_args_guard)
    .output()
    .expect("Failed to execute ssh command");
    tracing::info!(%name, %ip, "DONE: {}", output.status);
    io::stdout().write_all(&output.stdout).expect("Failed to write stdout to console");
    io::stderr().write_all(&output.stderr).expect("Failed to write stderr to console");
}

fn get_final_command(update_needed: bool, remote_command: &str) -> String {
    if update_needed {
        format!("{} && {}", get_pull_update_command(), remote_command)
    }
    else {
        String::from(remote_command)
    }
}

fn get_pull_update_command() -> String {
    String::from("cd C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations && git fetch && git reset --hard origin/main && git clean -fd")
}