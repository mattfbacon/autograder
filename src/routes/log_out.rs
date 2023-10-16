use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use sqlx::query;

use crate::extract::auth::{Token, User};
use crate::{error, State};

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
) -> Result<impl IntoResponse, Response> {
	if let Some(user) = user {
		query!("delete from sessions where user = ?", user.id)
			.execute(&state.database)
			.await
			.map_err(error::sqlx(Some(&user)))?;
	}

	Ok((Token::removal(), Redirect::to("/log-in")))
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/log-out", get(handler))
}
