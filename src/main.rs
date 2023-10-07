#![deny(
	absolute_paths_not_starting_with_crate,
	future_incompatible,
	keyword_idents,
	macro_use_extern_crate,
	meta_variable_misuse,
	missing_abi,
	missing_copy_implementations,
	non_ascii_idents,
	nonstandard_style,
	noop_method_call,
	pointer_structural_match,
	private_in_public,
	rust_2018_idioms,
	unused_qualifications
)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::unused_async)]
#![forbid(unsafe_code)]

use std::sync::Arc;

use once_cell::sync::Lazy;
use sqlx::{query, SqlitePool};

use crate::config::Config;
use crate::extract::auth;
use crate::resources::resources;
use crate::sandbox::Sandbox;
use crate::time::now;

mod config;
mod error;
mod extract;
mod model;
mod password;
mod resources;
mod routes;
mod sandbox;
mod template;
mod time;
mod util;

static CONFIG: Lazy<Config> = Lazy::new(Config::load);

pub struct State {
	database: SqlitePool,
	sandbox: Sandbox,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt::fmt()
		.with_env_filter(
			// Query logs are noisy and are logged at info level.
			tracing_subscriber::EnvFilter::new(
				[
					"info,sqlx::query=warn",
					std::env::var("RUST_LOG").as_deref().unwrap_or(""),
				]
				.join(","),
			),
		)
		.init();

	Lazy::force(&CONFIG);

	let database = SqlitePool::connect("sqlite://db.sqlite?mode=rwc")
		.await
		.expect("opening database");

	sqlx::migrate!()
		.run(&database)
		.await
		.expect("running migrations");

	tokio::spawn(clear_expired_tokens(database.clone()));

	let sandbox = sandbox::Sandbox::new().await;

	let state = Arc::new(State { database, sandbox });

	let app = axum::Router::new()
		.merge(routes::router().layer(error::method_not_allowed_layer()))
		.route_layer(auth::layer(Arc::clone(&state)))
		.fallback(error::not_found_handler)
		.with_state(state)
		.nest("/res", resources());

	let address = ([127, 0, 0, 1], 3000).into();

	tracing::info!("serving at {address}");
	axum::Server::bind(&address)
		.serve(app.into_make_service())
		.await
		.expect("running server");
}

async fn clear_expired_tokens(database: SqlitePool) {
	let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
	interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

	loop {
		interval.tick().await;
		tracing::debug!("clearing expired sessions");
		let now = now();
		let res = query!("delete from sessions where expiration < ?", now)
			.execute(&database)
			.await;
		match res {
			Ok(res) => tracing::debug!("successfully cleared {} sessions", res.rows_affected()),
			Err(error) => tracing::error!("error deleting expired sessions: {error}"),
		}
	}
}
