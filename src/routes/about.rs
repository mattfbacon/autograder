use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;

use crate::extract::auth::User;
use crate::model::Language;
use crate::template::page;
use crate::State;

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
) -> Response {
	let body = html! {
		h1 { "Versions and Compile Flags" }
		@for (i, version) in state.sandbox.versions().iter().enumerate() {
			@let language = Language::from_repr(i.try_into().unwrap()).unwrap();
			h2 { (language.name()) }
			pre { code { (version) } }
		}
	};

	page("About", user.as_ref(), &body).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/about", get(handler))
}
