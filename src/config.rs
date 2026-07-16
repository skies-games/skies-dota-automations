use std::env;
use dotenvy::dotenv;

pub struct Config {
	pub debug: bool,
	pub app_name: String,
	pub app_version: String,
	pub deployment_env: String,
	pub coordinator_server_ip: String,
	pub coordinator_server_port: u16,
	pub openobserve_endpoint: String,
	pub openobserve_credentials: String,
	pub openobserve_organization: String,
	pub openobserve_stream_name: String,
}

impl Config {
	pub fn build() -> Self {
		dotenv().ok();
		let debug = env::args().any(|arg| arg == "-d" || arg == "--debug");
		Self {
			debug,
			app_name: "skiesdota-automations".to_string(),
			app_version: "0.1.0".to_string(),
			deployment_env: "dev".to_string(),
			openobserve_organization: "default".to_string(),
			openobserve_stream_name: "skiesdota-automations".to_string(),
			coordinator_server_ip: env::var("COORDINATOR_SERVER_IP")
				.expect("COORDINATOR_SERVER_IP is not set"),
			coordinator_server_port: env::var("COORDINATOR_SERVER_PORT")
				.expect("COORDINATOR_SERVER_PORT is not set")
				.parse()
				.expect("COORDINATOR_SERVER_PORT must be a valid u16"),
			openobserve_endpoint: env::var("OPENOBSERVE_ENDPOINT")
				.expect("OPENOBSERVE_ENDPOINT is not set"),
			openobserve_credentials: env::var("OPENOBSERVE_CREDENTIALS")
				.expect("OPENOBSERVE_CREDENTIALS is not set"),
		}
	}
}