use std::sync::Arc;

use axum::extract;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;
use serde::Deserialize;
use sqlx::{query, query_scalar};

use super::pass_rate;
use crate::error::ErrorResponse;
use crate::extract::auth::User;
use crate::extract::if_post::IfPost;
use crate::model::{Language, ProblemId, SubmissionId, Tests};
use crate::template::{page, BannerKind};
use crate::time::{now, Timestamp};
use crate::util::{deserialize_textarea, s};
use crate::{error, State};

#[derive(Debug, Deserialize)]
struct Post {
	language: Language,
	#[serde(deserialize_with = "deserialize_textarea")]
	code: String,
}

fn can_edit(user: Option<&User>, problem_created_by: UserId) -> bool {
	user.is_some_and(|user| {
		user.permission_level >= PermissionLevel::Admin
			|| (user.permission_level >= PermissionLevel::ProblemAuthor && user.id == problem_created_by)
	})
}

async fn handle_post(
	state: &State,
	user: Option<&User>,
	problem_id: ProblemId,
	post: &Post,
) -> Result<SubmissionId, ErrorResponse> {
	let Some(user) = user else {
		return Err(ErrorResponse {
			status: StatusCode::UNAUTHORIZED,
			message: "You must be logged in to make submissions.".into(),
		});
	};

	let now = now();
	let submission_id = query_scalar!("insert into submissions (code, for_problem, submitter, language, submission_time, result) values (?, ?, ?, ?, ?, null) returning id", post.code, problem_id, user.id, post.language, now).fetch_one(&state.database).await.map_err(ErrorResponse::internal)?;

	Ok(submission_id)
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(problem_id): extract::Path<ProblemId>,
	IfPost(post): IfPost<extract::Form<Post>>,
) -> Result<Response, Response> {
	let error = if let Some(extract::Form(post)) = post {
		match handle_post(&state, user.as_ref(), problem_id, &post).await {
			Ok(submission_id) => {
				match crate::routes::submissions::do_judge(&state, submission_id).await {
					Ok(response) => return Ok(response),
					Err(error) => Some(error),
				}
			}
			Err(error) => Some(error),
		}
	} else {
		None
	};

	let Some(problem) = query!(
		r#"select name, description, problems.creation_time as "creation_time: Timestamp", users.id as "created_by_id!", users.display_name as created_by_name, (select count(*) from submissions where for_problem = problems.id) as "num_submissions!: i64", (select count(*) from submissions where for_problem = problems.id and result like 'o%') as "num_correct_submissions!: i64", tests as "tests: Tests" from problems inner join users on problems.created_by = users.id where problems.id = ?"#,
		problem_id,
	)
	.fetch_optional(&state.database)
	.await
	.map_err(error::internal(user.as_ref()))?
	else {
		return Err(error::not_found(user.as_ref()).await);
	};

	let (sample_input, sample_output) = problem.tests.cases().next().unwrap();

	let pass_rate = pass_rate(problem.num_submissions, problem.num_correct_submissions);

	let body = html! {
		p { b {
			"Created by " a href={"/users/"(problem.created_by_id)} { (problem.created_by_name) } " on " (problem.creation_time) " | " (problem.num_submissions) " submission" (s(problem.num_submissions))
			@if let Some(pass_rate) = pass_rate {
				", " (pass_rate) "% correct"
			}
		} }
		p { (problem.description) }
		h2 { "Sample input" }
		pre { code { (sample_input) } }
		h2 { "Sample output" }
		pre { code { (sample_output) } }

		hr;
		h2 { "Submit your solution" }
		form method="post" {
			label {
				"Language"
				select name="language" required {
					@for &language in Language::ALL {
						option value=(language.repr()) { (language.name()) }
					}
				}
			}
			label {
				"Code"
				textarea name="code" rows="10" required {}
			}
			input type="submit" value="Submit";
		}
	};

	let title = format!("Problem {problem_id}: {}", problem.name);
	let mut page = page(&title, user.as_ref(), &body);
	let status = error.as_ref().map_or(StatusCode::OK, |error| error.status);
	if let Some(error) = &error {
		page = page.with_banner(BannerKind::Error, &error.message);
	}
	Ok((status, page).into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/:id", get(handler).post(handler))
}
