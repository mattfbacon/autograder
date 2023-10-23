use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;
use sqlx::{query, query_scalar};

use super::DEFAULT_PAGE_SIZE;
use crate::extract::auth::Admin;
use crate::extract::pagination::RawPagination;
use crate::model::PermissionLevel;
use crate::template::page;
use crate::time::Timestamp;
use crate::{error, State};

async fn users(
	extract::State(state): extract::State<Arc<State>>,
	Admin(user): Admin,
	pagination: RawPagination,
) -> Result<Response, Response> {
	let pagination = pagination.with_default_page_size(DEFAULT_PAGE_SIZE);
	let limit = pagination.limit();
	let offset = pagination.offset();

	let num_users = query_scalar!(r#"select count(*) as "count: i64" from users"#)
		.fetch_one(&state.database)
		.await
		.map_err(error::sqlx(Some(&user)))?;

	let users =
		query!(r#"select id, username, display_name, creation_time as "creation_time: Timestamp", permission_level as "permission_level: PermissionLevel" from users order by id desc limit ? offset ?"#, limit, offset)
			.fetch_all(&state.database)
			.await
			.map_err(error::sqlx(Some(&user)))?;

	let body = html! {
		table {
			thead { tr {
				th { "ID" }
				th { "Username" }
				th { "Display name" }
				th { "Creation time" }
				th { "Permission level" }
			} }
			tbody { @for user in &users { tr {
				td { (user.id) }
				td { a href={"/user/"(user.id)} { (user.username) } }
				td { (user.display_name) }
				td { (user.creation_time) }
				td { (user.permission_level.name()) }
			} } }
		}
		@if users.is_empty() { p { "Nothing here..." } }
		(pagination.make_pager(num_users, ""))
	};

	Ok(page("Users", Some(&user), &body).into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/users", get(users))
}
