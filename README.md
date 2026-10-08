# Skies Dota Automations

Rust CLI that triggers Windows-side “bot” automations on multiple machines over SSH (using `sshpass.exe`) and coordinates “stop” via a TCP connection to a coordinator.

## What it does

1. Reads credentials and bot targets from `config.toml`.
2. Parses CLI args (`-a/--automation`, optional `-b/--bots`).
3. For each selected bot IP, runs a predefined remote command via SSH.
4. If `-a stop` is used, it does **not** SSH — it sends a small TCP message to the coordinator.

## Requirements

- Windows host where you run this CLI:
  - `sshpass.exe` available in `PATH` (see `install.ps1`)
  - (Optional) access to SSH on the bot machines
- Bot machines:
  - reachable by SSH (username/password in `config.toml`)
  - Windows installed with `PsExec.exe` and the expected repo/scripts on Desktop:
    - `C:\Users\%USERNAME%\Desktop\skies-dota`
    - `C:\Users\%USERNAME%\Desktop\skies-dota-bot-automations`
  - compatible Python scripts referenced by remote commands (for `start`, etc.)

## Setup

### 1) Install `sshpass`

```powershell
./install.ps1
```

### 2) Configure `config.toml`

Create `config.toml` (the repo has `example_config.toml`) in the working directory:

- `username` / `password`: SSH credentials for bot machines
- `[bots]`: map `bot name -> IP`

Bot range selection relies on parsing digits from keys that contain `bot` (example: `yahor_bot12` -> `12`).

### 3) Configure `.env` (OpenObserve + coordinator)

The binary uses environment variables via `dotenvy`. Create a `.env` alongside the executable with:

- `COORDINATOR_SERVER_IP`
- `COORDINATOR_SERVER_PORT` (u16)
- `OPENOBSERVE_ENDPOINT`
- `OPENOBSERVE_CREDENTIALS`

OpenTelemetry/Logging is configured to export logs to OpenObserve.

## Usage

### Common flags

- `-a`, `--automation <name>`: automation name
- `-b`, `--bots <range>`: optional bot range (`"7"` or `"1-5"`)
- `--data <value>`: extra arg for automations that need one (`set_game_mode`, `set_search_region`)
- `-d`, `--debug`: enable debug logging (via `Config::build()`)

If `-b/--bots` is omitted, it selects the min..max bot numbers found in `config.toml`.

### Automation names

Supported `-a/--automation` values:

- `start`
- `dota_launch`
- `dota_shutdown`
- `cancel_game_search`
- `set_game_mode` — requires `--data all_pick|turbo`
- `set_search_region` — requires `--data <region>` (`Russia`, `Europe West`, `Europe East`, `Japan`, `SE Asia`, `US East`, `US West`)
- `disconnect`
- `setup`
- `setup_items`
- `spoof`
- `install_deps`
- `vpn_status` — vpverst process + TUN `/1` catchalls
- `vpn_install_startup_tasks` — Vpverst + ping keepalive scheduled tasks
- `stop` (special case: sends a TCP message to coordinator, then exits)

VPN automations SSH into **`skies-dota-bot-automations\vpn\`** only (never `skies-dota-windows-automations`).
A matching copy of those scripts also lives in `windows-automations\powershell\` for manual host setup.

Examples:

```powershell
cargo run -- -a set_game_mode --data turbo -b 1-3
cargo run -- -a set_search_region --data "Europe West" -b 7
```

## Stop behavior (`-a stop`)

For `stop`, the tool computes the selected bot number range and sends a payload like:

`10:<bot1>,<bot2>,...`

over TCP to:

- `COORDINATOR_SERVER_IP:COORDINATOR_SERVER_PORT`

## Notes on remote execution

- Remote SSH command uses:
  - `StrictHostKeyChecking=no`
  - `UserKnownHostsFile=NUL` (to avoid Windows `known_hosts` races)
- The code currently always performs a “git update” step before running the remote command (the function returns `true` and has a commented-out git comparison).

## Dev commands

```powershell
cargo build
cargo run -- -a start -b 1-3
```

