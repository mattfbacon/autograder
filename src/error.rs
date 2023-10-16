use std::convert::Infallible;
use std::future::Future;

use axum::body::{Body, HttpBody};
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::Route;
use maud::html;
use tower_layer::Layer;
use tower_service::Service;

use crate::extract::auth::User;
use crate::template::page;

fn constraint_message(name: &str) -> Option<&'static str> {
	Some(match name {
		"problems_name_not_empty" => "Problem name cannot be empty.",
		"problems_name" => "Problem name can only contain printable characters.",
		"problems_description" => "Problem description can only contain printable characters.",
		"users_username_unique" => "That username is already taken.",
		"users_username" => "Username can only contain lowercase letters, numbers, and underscores.",
		"users_display_name_not_empty" => "Display name cannot be empty.",
		"users_display_name" => "Display name can only contain printable characters.",
		_ => return None,
	})
}

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

	pub fn sqlx(error: sqlx::Error) -> Self {
		if let sqlx::Error::Database(db_error) = &error {
			let message = db_error.message();
			if let Some((_, constraint_name)) = message.split_once("constraint failed: ") {
				if let Some(message) = constraint_message(constraint_name) {
					return Self::bad_request(message);
				}
			}
		}

		Self::internal(error)
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

	pub fn into_response_in_extractor(self, parts: &axum::http::request::Parts) -> Response {
		let user = parts.extensions.get::<User>();
		self.into_response(user)
	}
}

pub fn sqlx(user: Option<&User>) -> impl '_ + FnOnce(sqlx::Error) -> Response {
	move |error| ErrorResponse::sqlx(error).into_response(user)
}

pub async fn not_found(user: Option<&User>) -> Response {
	ErrorResponse::not_found().await.into_response(user)
}

#[track_caller]
pub fn fake_not_found(user: Option<&User>) -> impl Future<Output = Response> + '_ {
	let location = std::panic::Location::caller();
	async move {
		tracing::warn!(
			%location,
			?user,
			"user tried to access page without permission; pretending it doesn't exist",
		);
		not_found(user).await
	}
}

pub async fn not_found_handler(user: Option<User>) -> Response {
	not_found(user.as_ref()).await
}

async fn method_not_allowed_layer_inner(req: Request<Body>, next: Next<Body>) -> Response {
	let method = req.method().clone();
	let user = req.extensions().get::<User>().cloned();

	let mut response = next.run(req).await;

	// Detect a Method Not Allowed response from a `MethodRouter` (with an empty body) and replace it.
	// Doing this here is simpler than setting the fallback handler for every `MethodRouter` across the entire app.
	if response.status() == StatusCode::METHOD_NOT_ALLOWED
		&& response.body().size_hint().exact() == Some(0)
	{
		let error = ErrorResponse {
			status: StatusCode::METHOD_NOT_ALLOWED,
			message: format!("The {method} method is not supported for this route."),
		};
		// The default handler sets `Content-Length` manually (not sure why).
		// This will be a problem because obviously it will not be correct.
		response.headers_mut().remove("Content-Length");
		*response.body_mut() = error.into_response(user.as_ref()).into_body();
	}

	response
}

#[rustfmt::skip] // Rustfmt chokes on this big generic type.
pub fn method_not_allowed_layer() -> impl Layer<
	Route,
	Service = impl Service<Request<Body>, Response = Response, Future = impl Send, Error = Infallible> + Clone,
> + Clone {
	axum::middleware::from_fn(method_not_allowed_layer_inner)
}
