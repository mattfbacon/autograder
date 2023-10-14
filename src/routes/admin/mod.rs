use std::sync::Arc;

use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;

use crate::extract::auth::Admin;
use crate::template::page;
use crate::State;

mod run_sql;
mod users;

const DEFAULT_PAGE_SIZE: u32 = 30;

async fn admin(Admin(user): Admin) -> Response {
	let body = html! {
		p { a href="/submissions" { "View all submissions" } }
		p { a href="/admin/users" { "View all users" } }
		p { a href="/admin/sql" { "Run SQL" } }
	};

	page("Admin", Some(&user), &body).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(admin))
		.merge(run_sql::router())
		.merge(users::router());
	axum::Router::new().nest("/admin", router)
}
