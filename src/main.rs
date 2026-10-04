use std::fs;
use std::env::args;
use std::sync::Arc;
use std::io::{self, Write};
use std::collections::BTreeMap;
use std::process::exit;

use automations::config;
use automations::logger;

use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::io::AsyncWriteExt;

const SKIES_DOTA_BOT_AUTOMATIONS_PATH: &str = "C:\\Users\\%USERNAME%\\Desktop\\skies-dota-bot-automations";
const SKIES_DOTA_PATH: &str = "C:\\Users\\%USERNAME%\\Desktop\\skies-dota";

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

    if args.automation_name == "stop" {
        run_stop_script(&app_config, &automations_config, &args).await;
        return;
    }

    let update_needed = check_git_for_updates();
    let remote_command = Arc::new(get_remote_command(
        args.automation_name.as_str(),
        args.additional_data_path.as_deref(),
    ));
    run_automation(automations_config, args, remote_command, update_needed, app_config.debug).await;
}

fn parse_args() -> Args {
    let args_map: BTreeMap<&str, ArgKind> = BTreeMap::from([
        ("-a", ArgKind::AutomationName),
        ("--automation", ArgKind::AutomationName),
        ("-b", ArgKind::SelectedBots),
        ("--bots", ArgKind::SelectedBots),
        ("--data", ArgKind::AdditionalDataPath),
    ]);
    let args: Vec<String> = args().collect();
    let args_len = args.len();

    if args_len < 2 {
        tracing::error!("Usage: {} -a <automation_name> [-b <bot_range>] [-d|--debug]", args[0]);
        exit(1);
    }

    let mut args_struct: Args = Args::new();
    // args[0] is the binary; flags start at args[1] (e.g. -a dota_launch)
    // -d/--debug is a boolean (consumed by Config::build); skip it here
    let mut i = 1;
    while i < args_len {
        let flag = args[i].as_str();
        if flag == "-d" || flag == "--debug" {
            i += 1;
            continue;
        }
        if let Some(arg) = args_map.get(flag) {
            if i + 1 >= args_len {
                tracing::error!("Missing value for argument: {}", flag);
                exit(1);
            }
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
            i += 2;
        } else {
            tracing::error!("Invalid argument: {}", flag);
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

fn check_git_for_updates() -> bool {
    true
    // temporary disable git check for updates
    /*
    let local_commit_hash_output = std::process::Command::new("git")
    .args(&["rev-parse", "HEAD"])
    .output()
    .expect("Failed to execute: git rev-parse HEAD");
    assert!(local_commit_hash_output.status.success(), "Failed to execute: git rev-parse HEAD: {}", &local_commit_hash_output.status);
    let local_commit_hash = String::from_utf8(local_commit_hash_output.stdout).expect("Failed to convert stdout to string")
                            .split_whitespace()
                            .next().expect("Failed to get local commit hash")
                            .to_string();
    tracing::info!("local_commit_hash: {}", local_commit_hash);

    let remote_commit_hash_output = std::process::Command::new("git")
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
    */
}

/// PsExec -d returns the child PID as its exit code on success. Normalize to 0/1 via the "started" line.
fn psexec_detach(psexec_cmd: &str) -> String {
    format!(
        "({psexec_cmd} >%TEMP%\\psexec_out.txt 2>&1 & type %TEMP%\\psexec_out.txt & findstr /I /C:\"started\" %TEMP%\\psexec_out.txt >nul)"
    )
}

fn get_remote_command(command: &str, additional_data: Option<&str>) -> String {
    match command {
        "start" => psexec_detach(&format!(
            "PsExec.exe -s -d -i 1 -h -w {} -nobanner -accepteula {}\\Scripts\\python.exe {}\\src\\main.py --debug",
            SKIES_DOTA_PATH, SKIES_DOTA_PATH, SKIES_DOTA_PATH
        )),
        "dota_launch" => format!(
            "cd {} && Scripts\\pythonw.exe SetUpItems\\set_up_items.py && {}",
            SKIES_DOTA_BOT_AUTOMATIONS_PATH,
            psexec_detach(&format!(
                "PsExec.exe -s -d -i 1 -h -nobanner -accepteula {}\\dota_launch.bat",
                SKIES_DOTA_BOT_AUTOMATIONS_PATH
            ))
        ),
        "dota_shutdown" => format!("{}\\dota_shutdown.bat", SKIES_DOTA_BOT_AUTOMATIONS_PATH),
        "cancel_game_search" => psexec_detach(&format!(
            "PsExec.exe -s -d -i 1 -h -w {} -nobanner -accepteula {}\\Scripts\\pythonw.exe {}\\cancel_game_search.py",
            SKIES_DOTA_BOT_AUTOMATIONS_PATH, SKIES_DOTA_BOT_AUTOMATIONS_PATH, SKIES_DOTA_BOT_AUTOMATIONS_PATH
        )),
        "set_game_mode" => {
            let Some(game_mode) = additional_data else {
                tracing::error!("set_game_mode requires --data <all_pick|turbo>");
                exit(1);
            };
            psexec_detach(&format!(
                "PsExec.exe -s -d -i 1 -h -w {} -nobanner -accepteula {}\\Scripts\\pythonw.exe {}\\set_game_mode.py {}",
                SKIES_DOTA_BOT_AUTOMATIONS_PATH, SKIES_DOTA_BOT_AUTOMATIONS_PATH, SKIES_DOTA_BOT_AUTOMATIONS_PATH, game_mode
            ))
        }
        "set_search_region" => {
            let Some(region) = additional_data else {
                tracing::error!("set_search_region requires --data <region>");
                exit(1);
            };
            psexec_detach(&format!(
                "PsExec.exe -s -d -i 1 -h -w {} -nobanner -accepteula {}\\Scripts\\pythonw.exe {}\\set_search_region.py \"{}\"",
                SKIES_DOTA_BOT_AUTOMATIONS_PATH, SKIES_DOTA_BOT_AUTOMATIONS_PATH, SKIES_DOTA_BOT_AUTOMATIONS_PATH, region
            ))
        }
        "disconnect" => psexec_detach(&format!(
            "PsExec.exe -s -d -i 1 -h -nobanner -accepteula {}\\Scripts\\pythonw.exe {}\\disconnect.py",
            SKIES_DOTA_BOT_AUTOMATIONS_PATH, SKIES_DOTA_BOT_AUTOMATIONS_PATH
        )),
        "setup" => format!("cd {} && Scripts\\pythonw.exe SetUp\\set_up_dota.py", SKIES_DOTA_BOT_AUTOMATIONS_PATH),
        "setup_items" => format!("cd {} && Scripts\\pythonw.exe SetUpItems\\set_up_items.py", SKIES_DOTA_BOT_AUTOMATIONS_PATH),
        "spoof" => psexec_detach(&format!(
            "PsExec.exe -s -d -i 1 -h -nobanner -accepteula powershell.exe -NoProfile -ExecutionPolicy Bypass -File {}\\Spoof\\spoof.ps1",
            SKIES_DOTA_BOT_AUTOMATIONS_PATH
        )),
        // Use venv python directly (same as "start") — activate.bat is unreliable over non-interactive SSH.
        "install_deps" => format!(
            "cd {} && Scripts\\python.exe -m pip install -r requirements.txt",
            SKIES_DOTA_PATH
        ),
        _ => {
            tracing::error!(%command, "The remote command not found");
            exit(1);
        },
    }
}

fn bot_number_from_name(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    let pos = lower.find("bot")?;
    let digits: String = name[pos + 3..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn selected_bots_range(automations_config: &AutomationsConfig, args: &Args) -> (u16, u16) {
    if args.selected_bots.is_empty() {
        let numbers: Vec<u16> = automations_config
            .bots
            .keys()
            .filter_map(|name| bot_number_from_name(name))
            .collect();
        let bots_start = numbers.iter().copied().min().unwrap_or(1);
        let bots_end = numbers.iter().copied().max().unwrap_or(1);
        return (bots_start, bots_end);
    }
    let bots_start = *args.selected_bots.first().unwrap();
    let bots_end = *args.selected_bots.last().unwrap();
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

    println!("stop_script sent successfully");
    tracing::info!("stop_script sent successfully");
}

async fn run_automation(
    automations_config: Arc<AutomationsConfig>,
    args: Arc<Args>,
    remote_command: Arc<String>,
    update_needed: bool,
    debug: bool,
) {
    let (bots_start, bots_end) = selected_bots_range(&automations_config, &args);

    tracing::info!("Running automation on bots from range: {} to {}", bots_start, bots_end);

    let mut ssh_sessions = Vec::new();
    for (name, ip) in automations_config.bots.iter() {
        let Some(bot_number) = bot_number_from_name(name) else {
            tracing::warn!(%name, "Skipping bot (could not parse bot number from name)");
            continue;
        };
        if !(bot_number >= bots_start && bot_number <= bots_end) {
            tracing::info!(%name, %bot_number, "Skipping bot (not in selected range)");
            continue;
        }

        let automations_config = Arc::clone(&automations_config);
        let remote_command = Arc::clone(&remote_command);
        let ip = ip.clone();
        let name = name.clone();
        let automation_name = args.automation_name.clone();
        let ssh_session = tokio::spawn(async move {
            execute_ssh_command(
                automations_config,
                remote_command,
                update_needed,
                debug,
                &automation_name,
                &name,
                &ip,
            )
            .await
        });
        ssh_sessions.push(ssh_session);
    }

    let mut any_failed = false;
    for ssh_session in ssh_sessions {
        if !ssh_session.await.unwrap() {
            any_failed = true;
        }
    }
    if any_failed {
        exit(1);
    }
}

async fn execute_ssh_command(
    automations_config: Arc<AutomationsConfig>,
    remote_command: Arc<String>,
    update_needed: bool,
    debug: bool,
    automation_name: &str,
    name: &str,
    ip: &str,
) -> bool {
    // Skip known_hosts entirely — parallel SSH races on ~/.ssh/known_hosts.
    // Force password auth: Linux OpenSSH tries every ~/.ssh key first and hits
    // MaxAuthTries ("Too many authentication failures") before sshpass can send
    // the password from config.toml.
    let null_device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let sshpass_bin = if cfg!(windows) { "sshpass.exe" } else { "sshpass" };
    let sshpass_args = [
        format!("-p{}", automations_config.password),
        String::from("ssh"),
        String::from("-o"),
        String::from("StrictHostKeyChecking=no"),
        String::from("-o"),
        format!("UserKnownHostsFile={null_device}"),
        String::from("-o"),
        String::from("PreferredAuthentications=password"),
        String::from("-o"),
        String::from("PubkeyAuthentication=no"),
        String::from("-o"),
        String::from("ConnectTimeout=5"),
        format!("{}@{}", automations_config.username, ip),
        get_final_command(update_needed, automation_name, &*remote_command),
    ];

    tracing::info!(%name, %ip, "Connecting...");
    let output = Command::new(sshpass_bin)
        .args(&sshpass_args)
        .output()
        .await
        .expect("Failed to execute ssh command");

    let ok = output.status.success();
    if ok {
        println!("{name} ({ip}): OK");
        tracing::info!(%name, %ip, status = %output.status, "OK");
    } else {
        println!("{name} ({ip}): FAIL ({})", output.status);
        tracing::error!(%name, %ip, status = %output.status, "FAIL");
    }

    if debug {
        if !output.stdout.is_empty() {
            io::stdout().write_all(&output.stdout).expect("Failed to write stdout to console");
        }
        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(%name, %ip, %stderr, "ssh stderr");
            io::stderr().write_all(&output.stderr).expect("Failed to write stderr to console");
        }
    } else if !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(%name, %ip, %stderr, "ssh stderr");
    }

    ok
}

fn get_final_command(update_needed: bool, automation_name: &str, remote_command: &str) -> String {
    if !update_needed {
        return String::from(remote_command);
    }
    let path = if automation_name == "start" || automation_name == "install_deps" {
        SKIES_DOTA_PATH
    } else {
        SKIES_DOTA_BOT_AUTOMATIONS_PATH
    };
    //or use & instead of && if you don't want to block on a failed update from git
    format!("{} && {}", get_pull_update_command(path), remote_command) 
}

fn get_pull_update_command(path: &str) -> String {
    format!("cd {} && git fetch && git reset --hard origin/main", path)
}
