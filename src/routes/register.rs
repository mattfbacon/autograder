use std::sync::Arc;

use axum::extract;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use maud::html;
use serde::Deserialize;
use sqlx::query;

use crate::error::ErrorResponse;
use crate::extract::auth::User;
use crate::extract::if_post::IfPost;
use crate::extract::return_to::ReturnTo;
use crate::model::PermissionLevel;
use crate::template::{page, BannerKind};
use crate::time::now;
use crate::{password, State};

#[derive(Deserialize)]
struct Form {
	username: String,
	display_name: String,
	password: String,
}

async fn handle_post(state: &State, request: Form) -> Result<(), ErrorResponse> {
	// We do this now because password hashing is a bit computationally intensive.
	// If this were the only check, it would be prone to race conditions, but it's not.
	let user = query!("select id from users where username = ?", request.username)
		.fetch_optional(&state.database)
		.await
		.map_err(ErrorResponse::internal)?;

	if user.is_some() {
		return Err(ErrorResponse::bad_request("The username is already taken."));
	}

	let password_hash = password::Hash::new(&request.password);
	let creation_time = now();
	let permission_level = PermissionLevel::default();
	let res = query!(
		"insert into users (username, display_name, password, creation_time, permission_level) values (?, ?, ?, ?, ?)",
		request.username,
		request.display_name,
		password_hash,
		creation_time,
		permission_level,
	)
	.execute(&state.database)
	.await;

	match res {
		Err(sqlx::Error::Database(error))
			if error.kind() == sqlx::error::ErrorKind::UniqueViolation =>
		{
			return Err(ErrorResponse::bad_request("The username is already taken."));
		}
		res => {
			res.map_err(ErrorResponse::internal)?;
		}
	}

	Ok(())
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Query(return_to): extract::Query<ReturnTo>,
	IfPost(post): IfPost<extract::Form<Form>>,
) -> Response {
	let error = if let Some(extract::Form(post)) = post {
		match handle_post(&state, post).await {
			Ok(()) => return Redirect::to(&return_to.add_to_path("/login")).into_response(),
			Err(error) => Some(error),
		}
	} else {
		None
	};

	let status = error.as_ref().map_or(StatusCode::OK, |error| error.status);
	let body = html! {
		form method="post" {
			label { "Username" input name="username" type="text" required; }
			label { "Display name" input name="display_name" type="text" required; }
			label { "Password" input name="password" type="password" autocomplete="new-password" required; }
			input type="submit" value="Register";
		}
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
