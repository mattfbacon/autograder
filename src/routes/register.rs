use std::sync::Arc;

use axum::extract;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use maud::html;
use serde::Deserialize;
use sqlx::{query, query_scalar};

use crate::error::ErrorResponse;
use crate::extract::auth::User;
use crate::extract::if_post::IfPost;
use crate::extract::return_to::ReturnTo;
use crate::model::PermissionLevel;
use crate::template::{page, BannerKind};
use crate::time::now;
use crate::util::deserialize_non_empty;
use crate::State;

#[derive(Deserialize)]
struct Form {
	username: String,
	display_name: String,
	#[serde(default)]
	#[serde(deserialize_with = "deserialize_non_empty")]
	email: Option<String>,
	password: String,
}

async fn handle_post(state: &State, request: &Form) -> Result<(), ErrorResponse> {
	// We do this now because password hashing is a bit computationally intensive.
	// If this were the only check, it would be prone to race conditions, but it's not.
	let user = query!("select id from users where username = ?", request.username)
		.fetch_optional(&state.database)
		.await
		.map_err(ErrorResponse::sqlx)?;

	if user.is_some() {
		return Err(ErrorResponse::bad_request("The username is already taken."));
	}

	let creation_time = now();
	let permission_level = PermissionLevel::default();
	let id = query_scalar!(
		"insert into users (username, display_name, email, password, creation_time, permission_level) values (?, ?, ?, '', ?, ?) returning id",
		request.username,
		request.display_name,
		request.email,
		creation_time,
		permission_level,
	)
	.fetch_one(&state.database)
	.await
	.map_err(ErrorResponse::sqlx)?;

	let password_hash = crate::password::hash(&request.password);
	query!(
		"update users set password = ? where id = ?",
		password_hash,
		id,
	)
	.execute(&state.database)
	.await
	.map_err(ErrorResponse::sqlx)?;

	Ok(())
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Query(return_to): extract::Query<ReturnTo>,
	IfPost(post): IfPost<extract::Form<Form>>,
) -> Response {
	let post = post.map(|extract::Form(post)| post);
	let post = post.as_ref();
	let error = if let Some(post) = post {
		match handle_post(&state, post).await {
			Ok(()) => return Redirect::to(&return_to.add_to_path("/log-in")).into_response(),
			Err(error) => Some(error),
		}
	} else {
		None
	};

	let status = error.as_ref().map_or(StatusCode::OK, |error| error.status);
	let body = html! {
		form method="post" {
			label { "Username" input name="username" type="text" value=[post.map(|post| &post.username)] required; }
			label { "Display name" input name="display_name" type="text" value=[post.map(|post| &post.display_name)] required; }
			label { "Email (optional, for password reset)" input name="email" value=[post.and_then(|post| post.email.as_deref())] type="email"; }
			label { "Password" input name="password" type="password" autocomplete="new-password" value=[post.map(|post| &post.password)] required; }
			input type="submit" value="Register";
		}
		a href=(return_to.add_to_path("/log-in")) { "Or log in" }
	};

	let mut page = page("Register", user.as_ref(), &body);
	if let Some(error) = &error {
		page = page.with_banner(BannerKind::Error, &error.message);
	}
	(status, page).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/register", get(handler).post(handler))
}
