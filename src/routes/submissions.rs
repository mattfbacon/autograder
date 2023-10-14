use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::html;
use sqlx::query;

use crate::error::ErrorResponse;
use crate::extract::auth::User;
use crate::model::{Language, PermissionLevel, SubmissionId, UserId};
use crate::sandbox::{Test, TestResponse};
use crate::template::page;
use crate::time::{now, Timestamp};
use crate::{error, State};

pub async fn do_judge(
	state: &State,
	submission_id: SubmissionId,
) -> Result<Response, ErrorResponse> {
	let Some(submission) = query!(
		r#"select code, language as "language: Language", problems.time_limit as "time_limit: u32", problems.tests, problems.custom_judger from submissions inner join problems on submissions.for_problem = problems.id where submissions.id = ?"#,
		submission_id
	)
	.fetch_optional(&state.database)
	.await
	.map_err(ErrorResponse::internal)?
	else {
		return Err(ErrorResponse::not_found().await);
	};

	let response = state
		.sandbox
		.test(&Test {
			language: submission.language,
			time_limit: submission.time_limit,
			code: &submission.code,
			tests: &submission.tests,
			custom_judger: submission.custom_judger.as_deref(),
		})
		.await
		.map_err(ErrorResponse::internal)?;

	let now = now();

	query!(
		"update submissions set judged_time = ?, result = ? where id = ?",
		now,
		response,
		submission_id,
	)
	.execute(&state.database)
	.await
	.map_err(ErrorResponse::internal)?;

	Ok(Redirect::to(&format!("/submission/{submission_id}")).into_response())
}

#[derive(Debug, Clone, Copy)]
enum SubmissionPermissionLevel {
	None,
	View,
	Edit,
}

impl SubmissionPermissionLevel {
	pub fn can_view(self) -> bool {
		match self {
			Self::None => false,
			Self::View | Self::Edit => true,
		}
	}

	pub fn can_edit(self) -> bool {
		match self {
			Self::None | Self::View => false,
			Self::Edit => true,
		}
	}
}

fn permission_level(
	login_user: Option<&User>,
	submitter: UserId,
	problem_author: Option<UserId>,
) -> SubmissionPermissionLevel {
	let Some(login_user) = login_user else {
		return SubmissionPermissionLevel::None;
	};

	if login_user.permission_level >= PermissionLevel::Admin || login_user.id == submitter {
		return SubmissionPermissionLevel::Edit;
	}

	if problem_author.is_some_and(|problem_author| {
		login_user.permission_level >= PermissionLevel::ProblemAuthor && login_user.id == problem_author
	}) {
		return SubmissionPermissionLevel::View;
	}

	SubmissionPermissionLevel::None
}

async fn rejudge(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(submission_id): extract::Path<SubmissionId>,
) -> Result<Response, Response> {
	let Some(submission) = query!("select submitter, problems.created_by as problem_author from submissions inner join problems on submissions.for_problem = problems.id where submissions.id = ?", submission_id).fetch_optional(&state.database).await.map_err(error::internal(user.as_ref()))? else {
		return Err(error::not_found(user.as_ref()).await);
	};

	let permission_level = permission_level(
		user.as_ref(),
		submission.submitter,
		submission.problem_author,
	);

	if !permission_level.can_edit() {
		return Err(error::fake_not_found(user.as_ref()).await);
	}

	do_judge(&state, submission_id)
		.await
		.map_err(|error| error.into_response(user.as_ref()))
}

async fn delete(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(submission_id): extract::Path<SubmissionId>,
) -> Result<Response, Response> {
	let Some(submission) = query!("select submitter, for_problem, problems.created_by as problem_author from submissions inner join problems on submissions.for_problem = problems.id where submissions.id = ?", submission_id).fetch_optional(&state.database).await.map_err(error::internal(user.as_ref()))? else {
		return Err(error::not_found(user.as_ref()).await);
	};

	let permission_level = permission_level(
		user.as_ref(),
		submission.submitter,
		submission.problem_author,
	);

	if !permission_level.can_edit() {
		return Err(error::fake_not_found(user.as_ref()).await);
	}

	query!("delete from submissions where id = ?", submission_id)
		.execute(&state.database)
		.await
		.map_err(error::internal(user.as_ref()))?;
	Ok(Redirect::to(&format!("/problem/{}", submission.for_problem)).into_response())
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(submission_id): extract::Path<SubmissionId>,
) -> Result<Response, Response> {
	let Some(submission) = query!(r#"select code, for_problem as problem_id, problems.name as problem_name, problems.created_by as problem_author, submitter, users.display_name as submitter_name, language as "language: Language", submission_time as "submission_time: Timestamp", judged_time as "judged_time: Timestamp", result as "result: TestResponse" from submissions inner join problems on submissions.for_problem = problems.id inner join users on submissions.submitter = users.id where submissions.id = ?"#, submission_id).fetch_optional(&state.database).await.map_err(error::internal(user.as_ref()))? else {
		return Err(error::not_found(user.as_ref()).await);
	};

	let permission_level = permission_level(
		user.as_ref(),
		submission.submitter,
		submission.problem_author,
	);
	if !permission_level.can_view() {
		return Err(error::fake_not_found(user.as_ref()).await);
	}

	let body = html! {
		h1 { "Submission for " a href={"/problem/"(submission.problem_id)} { "Problem " (submission.problem_id) ": " (submission.problem_name) } }
		@if permission_level.can_edit() {
			form method="post" action={"/submission/"(submission_id)"/delete"} { input type="submit" value="Delete"; }
		}
		p { b {
			"By " (submission.submitter_name)
			" | Submitted at " (submission.submission_time)
			@if let Some(judged_time) = submission.judged_time {
				" | Judged at " (judged_time)
			}
		} }
		h2 { "Test Results" }
		@match &submission.result {
			Some(TestResponse::Ok(cases)) => {
				h3 { "Cases" }
				table {
					thead { tr {
						th { "#" }
						th { "Result" }
						th { "Time" }
					} }
					tbody { @for (i, case) in cases.iter().enumerate() { tr {
						td { (i + 1) }
						td { (case.kind.as_str()) }
						td { (case.time) " ms" }
					} } }
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

	let title = format!("Submission for Problem {}", submission.problem_id);
	let page = page(&title, user.as_ref(), &body);
	Ok(page.custom_title().into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(handler))
		.route("/delete", post(delete))
		.route("/rejudge", post(rejudge));
	axum::Router::new().nest("/submission/:id", router)
}
