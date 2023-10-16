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
	pub implicit_tls: Option<bool>,
}

impl Smtp {
	#[allow(clippy::match_same_arms /* Separate default case. */)]
	pub fn implicit_tls(&self) -> bool {
		self.implicit_tls.unwrap_or(match self.port {
			465 => true,
			25 | 587 => false,
			// Default.
			_ => false,
		})
	}
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
