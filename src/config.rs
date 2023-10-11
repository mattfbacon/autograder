use bindable::BindableAddr;

#[derive(serde::Deserialize)]
pub struct Config {
	/// Required for external links.
	pub external_url: String,
	pub admin_email: String,
	pub smtp: Smtp,
	pub address: BindableAddr,
}

#[derive(serde::Deserialize)]
pub struct Smtp {
	pub host: String,
	pub port: u16,
	pub username: String,
	pub password: String,
	#[serde(default = "false_")]
	pub implicit_tls: bool,
}

const fn false_() -> bool {
	false
}

const CONFIG_PATH: &str = "config.toml";

impl Config {
	pub fn load() -> Self {
		let raw = std::fs::read_to_string(CONFIG_PATH)
			.unwrap_or_else(|error| panic!("reading config from {CONFIG_PATH:?}: {error}"));
		toml::from_str(&raw)
			.unwrap_or_else(|error| panic!("reading config from {CONFIG_PATH:?}: {error}"))
	}
}
