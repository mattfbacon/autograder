use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use maud::html;

use crate::extract::auth::User;
use crate::template::page;

pub struct ErrorResponse {
	pub status: StatusCode,
	pub message: String,
}

impl ErrorResponse {
	pub fn internal<T: std::fmt::Debug>(error: T) -> Self {
		let id: u32 = rand::random();
		tracing::error!(?id, ?error, "internal error");
		Self {
			status: StatusCode::INTERNAL_SERVER_ERROR,
			message: format!(
				"The error has been logged under ID {id}. Contact the administrator with this ID."
			),
		}
	}

	pub fn bad_request<T: Into<String>>(reason: T) -> Self {
		ErrorResponse {
			status: StatusCode::BAD_REQUEST,
			message: reason.into(),
		}
	}

	pub async fn not_found() -> Self {
		let message = tokio::process::Command::new("fortune")
			.arg("-s")
			.output()
			.await
			.ok()
			.filter(|output| output.status.success())
			.and_then(|output| String::from_utf8(output.stdout).ok())
			.unwrap_or_else(|| "Whaddawha??".into());

		ErrorResponse {
			status: StatusCode::NOT_FOUND,
			message,
		}
	}

	pub fn into_response(self, user: Option<&User>) -> Response {
		let mnemonic = self
			.status
			.canonical_reason()
			.unwrap_or("An error occurred.");
		let body = html! {
			h1 { (self.status.as_str()) " " (mnemonic) }
			p.preserve-space { (&self.message) }
		};
		(self.status, page("Error!", user, &body).custom_title()).into_response()
	}

	pub fn into_response_in_extractor(self, parts: &mut axum::http::request::Parts) -> Response {
		let user = parts.extensions.get::<User>();
		self.into_response(user)
	}
}

pub fn internal<T: std::fmt::Debug>(user: Option<&User>) -> impl '_ + FnOnce(T) -> Response {
	move |error| ErrorResponse::internal(error).into_response(user)
}

pub async fn not_found(user: Option<&User>) -> Response {
	ErrorResponse::not_found().await.into_response(user)
}

pub async fn not_found_handler(user: Option<User>) -> Response {
	not_found(user.as_ref()).await
}
