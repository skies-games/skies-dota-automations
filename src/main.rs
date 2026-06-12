use std::fs;
use std::env::args;
use std::collections::HashMap;
use std::io::{self, Write};
use std::process::{exit, Command};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    username: String,
    password: String,
    github_pat: String,
    hosts: HashMap<String, Host>
}

#[derive(Debug, Deserialize)]
struct Host {
    ip: String,
    vms: HashMap<String, String>
}

#[derive(Debug)]
struct Args {
    automation_name: String,
    host_owner_name: String,
    selected_servers: Vec<u16>,
    selected_bots: Vec<u16>,
    additional_data_path: Option<String>
}

impl Args {
    fn new() -> Self {
        Self {
            automation_name: String::new(),
            host_owner_name: String::new(),
            selected_servers: Vec::new(),
            selected_bots: Vec::new(),
            additional_data_path: None,
        }
    }
}

enum ArgKind {
    AutomationName,
    HostOwnerName,
    SelectedServers,
    SelectedBots,
    AdditionalDataPath
}

#[tokio::main]
async fn main() {
    let config: Config = toml::from_str(&fs::read_to_string("config.toml").expect("Failed to read config.toml")).expect("Failed to parse TOML");
    let args: Args = parse_args();
    println!("args: {:?}", args);
    exit(0);
    let update_needed = check_git_for_updates();
    let remote_command = get_remote_command("launch_dota");
    let mut sshpass_args: Vec<String> = Vec::new();
    sshpass_args.push(format!("-p{}", config.password));
    sshpass_args.push(String::from("ssh"));
    
    let mut i = 0;
    println!("Running automations...");
    for (name, host) in config.hosts.iter() {
        println!("host: {}", name);
        for (name, ip) in host.vms.iter() {
            println!("vm : {}, ip: {}", name, ip);
            if let Some(arg) = sshpass_args.get(2) {
                sshpass_args[2] = format!("{}@{}", config.username, ip);
                sshpass_args[3] = construct_command(update_needed, remote_command.clone());
            }
            else {
                sshpass_args.push(format!("{}@{}", config.username, ip));
                sshpass_args.push(construct_command(update_needed, remote_command.clone()));
            }

            println!("sshpass_args: {:?}", sshpass_args);
            let output = Command::new("sshpass")
            .args(&sshpass_args)
            .output()
            .expect("ssh command failed to start");
            println!("status: {}", output.status);
            io::stdout().write_all(&output.stdout).expect("Failed to write stdout to console");
            io::stderr().write_all(&output.stderr).expect("Failed to write stderr to console");

        }

    }
}

fn parse_args() -> Args {
    let args_map: HashMap<&str, ArgKind> = HashMap::from([
        ("-a", ArgKind::AutomationName),
        ("--automation", ArgKind::AutomationName),
        ("-o", ArgKind::HostOwnerName),
        ("--owner", ArgKind::HostOwnerName),
        ("-s", ArgKind::SelectedServers),
        ("--servers", ArgKind::SelectedServers),
        ("-b", ArgKind::SelectedBots),
        ("--bots", ArgKind::SelectedBots),
        ("-d", ArgKind::AdditionalDataPath),
        ("--data", ArgKind::AdditionalDataPath),
    ]);
    let args: Vec<String> = args().collect();
    let args_len = args.len();

    if args_len < 2 {
        println!("Usage: {} <automation_name>", args[0]);
        exit(1);
    }

    let mut args_struct: Args = Args::new();
    for i in (1..args_len).step_by(2) {
        if let Some(arg) = args_map.get(args[i].as_str()) {
            match arg {
                ArgKind::AutomationName => {
                    println!("Automation name: {}", args[i + 1]);
                    args_struct.automation_name = args[i + 1].clone();
                }
                ArgKind::HostOwnerName => {
                    println!("Host owner name: {}", args[i + 1]);
                    args_struct.host_owner_name = args[i + 1].clone();
                }
                ArgKind::SelectedServers => {
                    println!("Selected servers: {}", args[i + 1]);
                    args_struct.selected_servers = parse_range(args[i + 1].as_str());
                }
                ArgKind::SelectedBots => {
                    println!("Selected bots: {}", args[i + 1]);
                    args_struct.selected_bots = parse_range(args[i + 1].as_str());
                }
                ArgKind::AdditionalDataPath => {
                    println!("Additional data path: {}", args[i + 1]);
                    args_struct.additional_data_path = Some(args[i + 1].clone());
                }
            }
        }
        else {
            println!("Invalid argument: {}", args[i]);
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
    .args(&["rev-parse", "bot-automations", "main"])
    .output()
    .expect("Failed to execute: git rev-parse bot-automations main");
    assert!(local_commit_hash_output.status.success(), "Failed to execute: git rev-parse bot-automations main: {}", &local_commit_hash_output.status);
    let local_commit_hash = String::from_utf8(local_commit_hash_output.stdout).expect("Failed to convert stdout to string")
                            .split_whitespace()
                            .next().expect("Failed to get local commit hash")
                            .to_string();
    println!("local_commit_hash: {}", local_commit_hash);

    let remote_commit_hash_output = Command::new("git")
    .args(&["ls-remote", "bot-automations", "-h", "refs/heads/main"])
    .output()
    .expect("Failed to execute: git ls-remote bot-automations -h refs/heads/main");
    assert!(remote_commit_hash_output.status.success(), "Failed to execute: git ls-remote bot-automations -h refs/heads/main: {}", &remote_commit_hash_output.status);
    let remote_commit_hash = String::from_utf8(remote_commit_hash_output.stdout).expect("Failed to convert stdout to string")
                            .split_whitespace()
                            .next().expect("Failed to get remote commit hash")
                            .to_string();
    println!("remote_commit_hash: {}", remote_commit_hash);

    local_commit_hash != remote_commit_hash
}

fn construct_command(update_needed: bool, remote_command: String) -> String {
    if update_needed {
        format!("{} && {}", get_pull_update_command(), remote_command)
    }
    else {
        remote_command
    }
}

fn get_remote_command(command: &str) -> String {
    match command {
        "launch_dota" => String::from("schtasks /run /tn \"start-dota\""),
        _ => {
            println!("The remote command \"{}\" not found", command);
            exit(1);
        },
    }
}

fn get_pull_update_command() -> String {
    String::from("cd C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations\\dota-power && git fetch && git reset --hard origin/main && git clean -fd")
}