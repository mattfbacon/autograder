use std::io::BufRead;
use std::process::Stdio;
use std::sync::Arc;

use regex::bytes::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::Language;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "command")]
enum Command<'a> {
	Test(&'a Test<'a>),
	Versions,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum CaseResult {
	Correct,
	Wrong,
	RuntimeError,
	Timeout,
}

impl CaseResult {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Correct => "Correct ✅",
			Self::Wrong => "Wrong ❌",
			Self::RuntimeError => "Runtime error 💥",
			Self::Timeout => "Timeout ⌛",
		}
	}
}

#[derive(Debug, Deserialize)]
pub enum TestResponse {
	Ok(Vec<CaseResult>),
	InvalidProgram(String),
}

#[derive(Debug)]
pub enum Error {
	Internal(String),
}

impl Error {
	fn internal<T: std::fmt::Display>(context: &str) -> impl '_ + FnOnce(T) -> Self {
		move |error| Self::Internal(format!("while {context}: {error}"))
	}
}

fn run_container(image_id: &str, raw_command: &[u8]) -> Result<Vec<u8>, Error> {
	let temp_dir = temp_dir::TempDir::new().map_err(Error::internal("creating temp dir"))?;
	let temp_dir = temp_dir.path();
	std::fs::write(temp_dir.join("command"), raw_command)
		.map_err(Error::internal("writing command to temp dir"))?;

	let temp_dir = temp_dir
		.to_str()
		.ok_or_else(|| Error::Internal("temporary directory path is not valid UTF-8".into()))?;
	let output = std::process::Command::new("docker")
		.args([
			"run",
			"--rm",
			"--memory=100m",
			"--network=none",
			"--mount",
			&format!("type=bind,source={temp_dir},destination=/input,readonly"),
			image_id,
		])
		.output()
		.map_err(Error::internal("running docker"))?;

	if !output.status.success() {
		return Err(Error::Internal(format!(
			"while running docker: got bad status {}. stderr: {}",
			output.status,
			String::from_utf8_lossy(&output.stderr),
		)));
	}

	Ok(output.stdout)
}

/// Returns the ID of the image.
#[tracing::instrument]
fn build_docker_image() -> Arc<str> {
	tracing::info!("invoking `docker build`; this may take a while");

	let mut child = std::process::Command::new("docker")
		.args(["build", "sandbox"])
		.stdout(Stdio::piped())
		.spawn()
		.expect("running docker builder");

	// We could cache the `Regex` in a `OnceCell` but this function is typically only called once so no need.
	let regex = Regex::new(r"Successfully built ([a-f0-9]+)").unwrap();

	let mut image_id = None;

	for line in std::io::BufReader::new(child.stdout.take().unwrap()).lines() {
		let line = line.unwrap();

		if let Some(captures) = regex.captures(line.as_bytes()) {
			let raw_id = captures
				.get(1)
				.expect("Could not get container ID from `docker build` output")
				.as_bytes();
			let id = std::str::from_utf8(raw_id)
				.expect("docker image ID is not valid UTF-8")
				.into();
			image_id = Some(id);
		}

		tracing::debug!("[docker] {line}");
	}

	let status = child.wait().expect("waiting for docker");
	assert!(
		status.success(),
		"`docker build` failed with status {status:?}",
	);

	let image_id = image_id.expect("failed to parse `docker build` output");

	tracing::info!(?image_id, "completed `docker build`");

	image_id
}

pub struct Sandbox {
	image_id: Arc<str>,
	versions: Box<[Box<str>]>,
}

#[derive(Debug, Serialize)]
pub struct Test<'a> {
	pub language: Language,
	pub memory_limit: u32,
	pub time_limit: u32,
	pub code: &'a str,
	pub tests: &'a str,
}

impl Sandbox {
	pub async fn new() -> Self {
		tracing::info!("ensuring docker image is built");
		let image_id = tokio::task::spawn_blocking(build_docker_image)
			.await
			.unwrap();

		let mut ret = Self {
			image_id,
			versions: Box::new([]),
		};

		tracing::debug!("getting versions from container");
		ret.versions = ret
			.run_command(Command::Versions)
			.await
			.expect("running Versions command");

		ret
	}

	async fn run_command<T: DeserializeOwned>(&self, command: Command<'_>) -> Result<T, Error> {
		let image_id = Arc::clone(&self.image_id);

		let mut command_buf = Vec::new();
		ciborium::into_writer(&command, &mut command_buf)
			.map_err(Error::internal("serializing runner command"))?;

		let output = tokio::task::spawn_blocking(move || run_container(&image_id, &command_buf))
			.await
			// This simply propagates panics from inside the handler.
			.unwrap()?;

		ciborium::from_reader(output.as_slice())
			.map_err(Error::internal("deserializing response from runner"))
	}

	pub async fn test(&self, test: &Test<'_>) -> Result<TestResponse, Error> {
		self.run_command(Command::Test(test)).await
	}

	/// The indices of these strings indicate the language that they are associated with.
	pub fn versions(&self) -> &[Box<str>] {
		&self.versions
	}
}
