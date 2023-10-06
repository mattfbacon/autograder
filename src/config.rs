#[derive(serde::Deserialize)]
pub struct Config {
	pub admin_email: String,
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
