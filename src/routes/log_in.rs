use std::sync::Arc;

use axum::extract;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use maud::html;
use serde::Deserialize;
use sqlx::query;

use crate::error::ErrorResponse;
use crate::extract::auth::{self, Token, User};
use crate::extract::if_post::IfPost;
use crate::extract::return_to::ReturnTo;
use crate::template::{page, BannerKind};
use crate::State;

#[derive(Deserialize)]
struct Form {
	username: String,
	password: String,
}

async fn handle_post(state: &State, form: Form) -> Result<Token, ErrorResponse> {
	let entry = query!(
		r#"select id as "id!", password as hash from users where username = ?"#,
		form.username,
	)
	.fetch_optional(&state.database)
	.await
	.map_err(ErrorResponse::internal)?;

	if let Some(entry) = entry {
		if bcrypt::verify(&form.password, &entry.hash).map_err(ErrorResponse::internal)? {
			let token = auth::log_in(state, entry.id).await?;
			return Ok(token);
		}
	}

	Err(ErrorResponse::bad_request(
		"Username or password is incorrect.",
	))
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Query(return_to): extract::Query<ReturnTo>,
	IfPost(post): IfPost<extract::Form<Form>>,
) -> Response {
	let error = if let Some(extract::Form(post)) = post {
		match handle_post(&state, post).await {
			Ok(token) => return (&token, Redirect::to(return_to.path())).into_response(),
			Err(error) => Some(error),
		}
	} else {
		None
	};

	let status = error.as_ref().map_or(StatusCode::OK, |error| error.status);
	let body = html! {
		h1 { "Log In" }
		form method="post" {
			label for="username" { "Username" input id="username" name="username" type="text" autocomplete="username" required; }
			label for="password" { "Password" input id="password" name="password" type="password" autocomplete="current-password" required; }
			input type="submit" value="Log in";
		}
	};

	let mut page = page("Log In", user.as_ref(), &body);
	if let Some(error) = &error {
		page = page.with_banner(BannerKind::Error, &error.message);
	}
	(status, page).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/login", get(handler).post(handler))
}
