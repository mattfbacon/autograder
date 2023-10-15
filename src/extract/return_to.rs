use std::borrow::Cow;

#[derive(serde::Deserialize)]
pub struct ReturnTo {
	#[serde(rename = "returnto")]
	return_to: Option<String>,
}

pub fn add_to_path(path: &str, return_to: &str) -> String {
	format!(
		"{path}?returnto={}",
		crate::util::encode_query(return_to.as_bytes()),
	)
}

impl ReturnTo {
	pub fn add_to_path<'a>(&self, path: &'a str) -> Cow<'a, str> {
		self
			.return_to
			.as_ref()
			.map_or(path.into(), |return_to| add_to_path(path, return_to).into())
	}

	pub fn path(&self) -> &str {
		self.return_to.as_deref().unwrap_or("/")
	}
}
