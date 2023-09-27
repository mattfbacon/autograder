use std::sync::Arc;

use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;

use crate::extract::auth::User;
use crate::template::page;
use crate::State;

async fn root(user: Option<User>) -> Response {
	let body = html! {
		@if let Some(user) = &user {
			p { "Welcome, " (user.display_name) }
		} @else {
			p { "Hello! Please " a href="/login" { "log in" } " or " a href="/register" { "register" } "." }
		}
	};
	page("Dashboard", user.as_ref(), &body).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/", get(root))
}
