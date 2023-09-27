use std::sync::Arc;

use crate::State;

mod about;
mod admin;
mod log_in;
mod log_out;
mod problems;
mod register;
mod root;
mod submissions;
mod users;

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new()
		.merge(about::router())
		.merge(admin::router())
		.merge(log_in::router())
		.merge(log_out::router())
		.merge(problems::router())
		.merge(register::router())
		.merge(root::router())
		.merge(submissions::router())
		.merge(users::router())
}
