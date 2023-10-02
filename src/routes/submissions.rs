use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::html;
use sqlx::query;

use crate::error::ErrorResponse;
use crate::extract::auth::User;
use crate::model::{Language, PermissionLevel, SubmissionId};
use crate::sandbox::{Test, TestResponse};
use crate::template::page;
use crate::time::Timestamp;
use crate::{error, State};

pub async fn do_judge(
	state: &State,
	submission_id: SubmissionId,
) -> Result<Response, ErrorResponse> {
	let Some(submission) = query!(
		r#"select code, language as "language: Language", problems.memory_limit, problems.time_limit, problems.tests from submissions inner join problems on submissions.for_problem = problems.id where submissions.id = ?"#,
		submission_id
	)
	.fetch_optional(&state.database)
	.await
	.map_err(ErrorResponse::internal)?
	else {
		return Err(ErrorResponse::not_found().await);
	};

	// These intentionally convert negative values to `u32::MAX`, because those values are interpreted to mean "no limit".
	let memory_limit = submission.memory_limit.try_into().unwrap_or(u32::MAX);
	let time_limit = submission.time_limit.try_into().unwrap_or(u32::MAX);

	let response = state
		.sandbox
		.test(&Test {
			language: submission.language,
			memory_limit,
			time_limit,
			code: &submission.code,
			tests: &submission.tests,
		})
		.await
		.map_err(ErrorResponse::internal)?;

	query!(
		"update submissions set result = ? where id = ?",
		response,
		submission_id
	)
	.execute(&state.database)
	.await
	.map_err(ErrorResponse::internal)?;

	Ok(Redirect::to(&format!("/submission/{submission_id}")).into_response())
}

async fn rejudge(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(submission_id): extract::Path<SubmissionId>,
) -> Response {
	do_judge(&state, submission_id)
		.await
		.unwrap_or_else(|error| error.into_response(user.as_ref()))
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(submission_id): extract::Path<SubmissionId>,
) -> Result<Response, Response> {
	let Some(submission) = query!(r#"select code, for_problem as problem_id, problems.name as problem_name, problems.created_by as problem_author, submitter, users.display_name as submitter_name, language as "language: Language", submission_time as "submission_time: Timestamp", result as "result: TestResponse" from submissions inner join problems on submissions.for_problem = problems.id inner join users on submissions.submitter = users.id where submissions.id = ?"#, submission_id).fetch_optional(&state.database).await.map_err(error::internal(user.as_ref()))? else {
		return Err(error::not_found(user.as_ref()).await);
	};

	let can_access = user.as_ref().is_some_and(|user| {
		// Administrator.
		user.permission_level >= PermissionLevel::Admin
		// Problem author who created the problem.
		|| (user.permission_level >= PermissionLevel::ProblemAuthor
			&& user.id == submission.problem_author)
		// User who made this submission.
		|| (user.id == submission.submitter)
	});
	if !can_access {
		return Err(error::not_found(user.as_ref()).await);
	}

	let body = html! {
		h1 { "Submission for " a href={"/problem/"(submission.problem_id)} { "Problem " (submission.problem_id) ": " (submission.problem_name) } }
		p { b { "By " (submission.submitter_name) " | Submitted at " (submission.submission_time) } }
		h2 { "Test Results" }
		@match &submission.result {
			Some(TestResponse::Ok(cases)) => {
				h3 { "Cases" }
				ol {
					@for case in cases {
						li { (case.as_str()) }
					}
				}
			},
			Some(TestResponse::InvalidProgram(reason)) => {
				p { "Program was invalid." }
				pre { code { (reason) } }
			},
			None => p { "Program not yet judged." },
		}
		form method="post" action={"/submission/"(submission_id)"/rejudge"} {
			input type="submit" value="Rejudge";
		}
		h2 { "Code" }
		p { "Language: " (submission.language.name()) }
		pre { code { (submission.code) } }
	};

	Ok(page("Submission", user.as_ref(), &body).into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(handler))
		.route("/rejudge", post(rejudge));
	axum::Router::new().nest("/submission/:id", router)
}
