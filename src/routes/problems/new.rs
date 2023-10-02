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
pub struct Problem {
	pub name: String,
	#[serde(deserialize_with = "deserialize_textarea")]
	pub description: String,

	pub time_limit: u32,
	pub memory_limit: u32,
	#[serde(default)]
	pub visible: bool,

	#[serde(deserialize_with = "deserialize_textarea")]
	pub tests: String,
}

const DEFAULT_TIME_LIMIT: u32 = 1000;
const DEFAULT_MEMORY_LIMIT: u32 = 8;

const EXAMPLE_TESTS: &str = "\
first input
--
first output
===
second input
--
second output
";

async fn handle_post(
	state: &State,
	user: &User,
	post: &Problem,
) -> Result<ProblemId, ErrorResponse> {
	Tests::validate(&post.tests).map_err(|error| ErrorResponse::bad_request(error.to_string()))?;
	let now = now();
	let id = query_scalar!("insert into problems (name, description, time_limit, memory_limit, visible, tests, creation_time, created_by) values (?, ?, ?, ?, ?, ?, ?, ?) returning id", post.name, post.description, post.time_limit, post.memory_limit, post.visible, post.tests, now, user.id).fetch_one(&state.database).await.map_err(ErrorResponse::internal)?;
	Ok(id)
}

pub fn problem_form(old: Option<&Problem>) -> maud::Markup {
	html! {
		label {
			"Name"
			input type="text" required name="name" value=[old.map(|post| &post.name)] {}
		}
		label {
			"Description"
			textarea required name="description" rows="4" { (old.map_or("", |post| &post.description)) }
		}
		label {
			"Time limit (milliseconds)"
			input type="number" required name="time_limit" value=(old.map_or(DEFAULT_TIME_LIMIT, |post| post.time_limit));
		}
		label {
			"Memory limit (MB)"
			input type="number" required name="memory_limit" value=(old.map_or(DEFAULT_MEMORY_LIMIT, |post| post.memory_limit));
		}
		label {
			"Visible"
			input type="checkbox" name="visible" value="true" checked[old.map_or(true, |post| post.visible)];
		}
		label {
			"Tests"
			textarea required name="tests" placeholder=(EXAMPLE_TESTS) rows="15" { (old.map_or("", |post| post.tests.trim())) }
			details {
				summary { "How to write tests" }
				p { "Separate test cases with three equals signs on their own line." }
				p { "Separate input and output in a test case with two dashes on their own line." }
				p { "Example:"}
				pre { code { (EXAMPLE_TESTS) } }
				p { "The first test case will be shown as an example on the problem page." }
			}
		}
	}
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	ProblemAuthor(user): ProblemAuthor,
	IfPost(post): IfPost<extract::Form<Problem>>,
) -> Response {
	let post = post.map(|extract::Form(post)| post);
	let post = post.as_ref();

	let error = if let Some(post) = post {
		match handle_post(&state, &user, post).await {
			Ok(id) => return Redirect::to(&format!("/problem/{id}")).into_response(),
			Err(error) => Some(error),
		}
	} else {
		None
	};

	let body = html! {
		form method="post" {
			(problem_form(post))
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
