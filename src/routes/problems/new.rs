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

fn deserialize_optional_textarea<'de, D: serde::de::Deserializer<'de>>(
	de: D,
) -> Result<Option<String>, D::Error> {
	deserialize_textarea(de).map(|v| Some(v).filter(|v| !v.is_empty()))
}

#[derive(Debug, serde::Deserialize)]
pub struct Problem {
	pub name: String,
	#[serde(deserialize_with = "deserialize_textarea")]
	pub description: String,

	pub time_limit: u32,
	#[serde(default)]
	pub visible: bool,

	#[serde(deserialize_with = "deserialize_textarea")]
	pub tests: String,

	#[serde(deserialize_with = "deserialize_optional_textarea")]
	pub custom_judger: Option<String>,
}

const DEFAULT_TIME_LIMIT: u32 = 1000;

const EXAMPLE_TESTS: &str = "\
first input
--
first output
===
second input
--
second output
";

const EXAMPLE_CUSTOM_JUDGER: &str = "\
# i: case index, zero-indexed.
# case_input, expected_output, actual_output: trimmed and normalized to unix newlines.
def judge(i: int, case_input: str, expected_output: str, actual_output: str) -> bool:
  # This is equivalent to the default judger:
  return actual_output == expected_output
";

async fn handle_post(
	state: &State,
	user: &User,
	post: &Problem,
) -> Result<ProblemId, ErrorResponse> {
	Tests::validate(&post.tests).map_err(|error| ErrorResponse::bad_request(error.to_string()))?;
	if let Some(judger) = &post.custom_judger {
		state
			.sandbox
			.validate_judger(judger)
			.await
			.map_err(ErrorResponse::internal)?
			.map_err(|error| {
				ErrorResponse::bad_request(format!("Custom judger failed validation: {error}"))
			})?;
	}

	let now = now();
	let id = query_scalar!("insert into problems (name, description, time_limit, visible, tests, custom_judger, creation_time, created_by) values (?, ?, ?, ?, ?, ?, ?, ?) returning id", post.name, post.description, post.time_limit, post.visible, post.tests, post.custom_judger, now, user.id).fetch_one(&state.database).await.map_err(ErrorResponse::sqlx)?;
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
			"Visible"
			input type="checkbox" name="visible" value="true" checked[old.map_or(true, |post| post.visible)];
		}
		label {
			"Tests"
			textarea required name="tests" placeholder=(EXAMPLE_TESTS) rows="15" cols="35" { (old.map_or("", |post| post.tests.trim())) }
		}
		details {
			summary { "How to write tests" }
			div.details {
				p { "Separate test cases with three equals signs on their own line." }
				p { "Separate input and output in a test case with two dashes on their own line." }
				p { "Example:"}
				pre { code { (EXAMPLE_TESTS) } }
				p { "The first test case will be shown as an example on the problem page." }
			}
		}
		label {
			"Custom Judger"
			textarea name="custom_judger" placeholder="(Empty = normal judging)" rows="15" cols="35" { (old.and_then(|post| post.custom_judger.as_deref()).map_or("", str::trim)) }
		}
		details {
			summary { "How to write a custom judger" }
			div.details {
				p { "Custom judgers are Python scripts." }
				p { "Export a function " code { "judge" } " with the following signature:" }
				pre { code { (EXAMPLE_CUSTOM_JUDGER) } }
				p { "You may wish to copy this into the Custom Judger field as a starting point." }
				p { "Your judger will undergo a limited amount of validation. You should make a test submission." }
				p { "If the judger throws an exception, the submission will be marked as an invalid program. If you are doing things like " code { "int(actual_output)" } ", you probably want to wrap the body in a try-except that returns False if an exception occurs." }
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
