use std::fs;
use std::process::Command;
use std::io::{self, Write};
use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    username: String,
    password: String,
    github_PAT: String,
    hosts: HashMap<String, Host>
}

#[derive(Debug, Deserialize)]
struct Host {
    ip: String,
    vms: HashMap<String, String>
}

fn main() {
    let file = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    let config: Config = toml::from_str(&file).expect("Failed to parse TOML");
    
    let mut sshpass_args: Vec<String> = Vec::new();
    sshpass_args.push(format!("-p{}", config.password));
    sshpass_args.push("ssh".to_string());
    //sshpass_args.push(format!("{}@", config.username));

    for (name, host) in config.hosts.iter() {
        println!("host: {}", name);
        for (name, ip) in host.vms.iter() {
            println!("vm: {}, ip: {}", name, ip);
        }
    }
    /*
    sshpass_args.push("floger@100.100.2.1");
    sshpass_args.push("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -ExecutionPolicy Bypass -File C:\\Users\\floger\\Desktop\\packages_install.ps1");

    let output = Command::new("sshpass")
    .args(sshpass_args)
    .output()
    .expect("ssh command failed to start");
    println!("status: {}", output.status);
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    Ok(())
    */
}
