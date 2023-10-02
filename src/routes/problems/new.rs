use std::sync::Arc;

use axum::extract;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use maud::html;
use sqlx::query_scalar;

use crate::error::ErrorResponse;
use crate::extract::auth::{ProblemAuthor, User};
use crate::extract::if_post::IfPost;
use crate::model::{ProblemId, Tests};
use crate::template::{page, BannerKind};
use crate::time::now;
use crate::util::deserialize_textarea;
use crate::State;

#[derive(Debug, serde::Deserialize)]
struct Form {
	title: String,
	#[serde(deserialize_with = "deserialize_textarea")]
	description: String,

	time_limit: u32,
	memory_limit: u32,
	#[serde(default)]
	visible: bool,

	#[serde(deserialize_with = "deserialize_textarea")]
	tests: String,
}

const EXAMPLE_TESTS: &str = "\
first input
--
first output
===
second input
--
second output
";

async fn handle_post(state: &State, user: &User, post: &Form) -> Result<ProblemId, ErrorResponse> {
	Tests::validate(&post.tests).map_err(|error| ErrorResponse::bad_request(error.to_string()))?;
	let now = now();
	let id = query_scalar!("insert into problems (name, description, time_limit, memory_limit, visible, tests, creation_time, created_by) values (?, ?, ?, ?, ?, ?, ?, ?) returning id", post.title, post.description, post.time_limit, post.memory_limit, post.visible, post.tests, now, user.id).fetch_one(&state.database).await.map_err(ErrorResponse::internal)?;
	Ok(id)
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	ProblemAuthor(user): ProblemAuthor,
	IfPost(post): IfPost<extract::Form<Form>>,
) -> Response {
	let post = post.map(|extract::Form(post)| post);
	let post = post.as_ref();

	let error = if let Some(post) = post {
		match handle_post(&state, &user, post).await {
			Ok(id) => return Redirect::to(&format!("/problems/{id}")).into_response(),
			Err(error) => Some(error),
		}
	} else {
		None
	};

	let body = html! {
		form method="post" {
			label {
				"Title"
				input type="text" required name="title" value=[post.map(|post| &post.title)] {}
			}
			label {
				"Description"
				textarea required name="description" rows="4" { (post.map_or("", |post| &post.description)) }
			}
			label {
				"Time limit (milliseconds)"
				input type="number" required name="time_limit" value=[post.map(|post| &post.time_limit)];
			}
			label {
				"Memory limit (MB)"
				input type="number" required name="memory_limit" value=[post.map(|post| &post.memory_limit)];
			}
			label {
				"Visible"
				input type="checkbox" name="visible" value="true" checked[post.map_or(true, |post| post.visible)];
			}
			label {
				"Tests"
				textarea required name="tests" placeholder=(EXAMPLE_TESTS) rows="8" { (post.map_or("", |post| &post.tests)) }
			}
			details {
				summary { "How to write tests" }
				p { "Separate test cases with three equals signs on their own line." }
				p { "Separate input and output in a test case with two dashes on their own line." }
				p { "Example:"}
				pre { code { (EXAMPLE_TESTS) } }
			}
			input type="submit" value="Create";
		}
	};

	let mut page = page("New Problem", Some(&user), &body);
	let status = error.as_ref().map_or(StatusCode::OK, |error| error.status);
	if let Some(error) = &error {
		page = page.with_banner(BannerKind::Error, &error.message);
	}
	(status, page).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/new", get(handler).post(handler))
}
