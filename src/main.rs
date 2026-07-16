use std::fs;
use std::env::args;
use std::sync::Arc;
use std::io::{self, Write};
use std::collections::BTreeMap;
use std::process::{exit, Command};

use automations::config;
use automations::logger;

use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
struct AutomationsConfig {
    username: String,
    password: String,
    bots: BTreeMap<String, String>,
}

#[derive(Debug)]
struct Args {
    automation_name: String,
    selected_bots: Vec<u16>,
    additional_data_path: Option<String>,
}

impl Args {
    fn new() -> Self {
        Self {
            automation_name: String::new(),
            selected_bots: Vec::new(),
            additional_data_path: None,
        }
    }
}

enum ArgKind {
    AutomationName,
    SelectedBots,
    AdditionalDataPath,
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

    if args.automation_name == "stop_script" {
        run_stop_script(&app_config, &automations_config, &args).await;
        return;
    }

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
        ("-b", ArgKind::SelectedBots),
        ("--bots", ArgKind::SelectedBots),
        ("-d", ArgKind::AdditionalDataPath),
        ("--data", ArgKind::AdditionalDataPath),
    ]);
    let args: Vec<String> = args().collect();
    let args_len = args.len();

    if args_len < 2 {
        tracing::error!("Usage: {} -a <automation_name> [-b <bot_range>]", args[0]);
        exit(1);
    }

    let mut args_struct: Args = Args::new();
    for i in (1..args_len).step_by(2) {
        if let Some(arg) = args_map.get(args[i].as_str()) {
            match arg {
                ArgKind::AutomationName => {
                    args_struct.automation_name = args[i + 1].clone();
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
        "start" => String::from("cd C:\\Users\\%USERNAME%\\Desktop\\skies-dota && \"venv/Scripts/activate.bat\" && python src/main.py"),
        "dota_launch" => String::from("\"C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations\\dota_launch.bat\""),
        "dota_shutdown" => String::from("\"C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations\\dota_shutdown.bat\""),
        "cancel_game_search" => String::from("cd C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations && \"venv/Scripts/activate.bat\" && python cancel_game_search.py"),
        "disconnect" => String::from("cd C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations && \"venv/Scripts/activate.bat\" && python disconnect.py"),
        _ => {
            tracing::error!(%command, "The remote command not found");
            exit(1);
        },
    }
}

fn selected_bots_range(automations_config: &AutomationsConfig, args: &Args) -> (u16, u16) {
    // if no selected bots, use the last bot number
    let bots_end = args.selected_bots
        .last().copied()
        .unwrap_or_else(|| automations_config.bots.len() as u16);
    // if no selected bots, use the first bot number
    let bots_start = args.selected_bots
                    .first().copied()
                    .unwrap_or(1);
    //and it means, if the user didn't select any bots, we run the automation on all the bots
    (bots_start, bots_end)
}

async fn run_stop_script(
    app_config: &config::Config,
    automations_config: &AutomationsConfig,
    args: &Args,
) {
    let (bots_start, bots_end) = selected_bots_range(automations_config, args);
    let affected_bots: Vec<String> = (bots_start..=bots_end).map(|n| n.to_string()).collect();
    let message = format!("10:{}", affected_bots.join(","));
    tracing::info!(%message, "Sending stop_script user command to coordinator for bots {}-{}", bots_start, bots_end);

    let mut stream = TcpStream::connect((
        app_config.coordinator_server_ip.as_str(),
        app_config.coordinator_server_port,
    ))
    .await
    .expect("Failed to connect to coordinator");

    let payload = message.as_bytes();
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await
        .expect("Failed to write message length to coordinator");
    stream
        .write_all(payload)
        .await
        .expect("Failed to write message to coordinator");
    stream
        .shutdown()
        .await
        .expect("Failed to shutdown coordinator connection");

    tracing::info!("stop_script sent successfully");
}

async fn run_automation(
    automations_config: Arc<AutomationsConfig>,
    args: Arc<Args>,
    sshpass_args: Arc<Mutex<Vec<String>>>,
    remote_command: Arc<String>,
    update_needed: bool,
) {
    let (bots_start, bots_end) = selected_bots_range(&automations_config, &args);

    tracing::info!("Running automation on bots from range: {} to {}", bots_start, bots_end);

    let mut bot_number = 1;
    let mut ssh_sessions = Vec::new();
    for (name, ip) in automations_config.bots.iter() {
        if !(bot_number >= bots_start && bot_number <= bots_end) {
            tracing::info!(%name, "Skipping bot (not in selected range)");
            bot_number += 1;
            continue;
        }

        let automations_config = Arc::clone(&automations_config);
        let sshpass_args = Arc::clone(&sshpass_args);
        let remote_command = Arc::clone(&remote_command);
        let ip = ip.clone();
        let name = name.clone();
        let ssh_session = tokio::spawn(async move {
            execute_ssh_command(automations_config, sshpass_args, remote_command, update_needed, &name, &ip).await;
        });
        ssh_sessions.push(ssh_session);
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
    ip: &str,
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
