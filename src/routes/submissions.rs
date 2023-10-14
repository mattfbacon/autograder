use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::html;
use sqlx::{query, query_scalar};

use crate::error::ErrorResponse;
use crate::extract::auth::{Admin, User};
use crate::extract::pagination::RawPagination;
use crate::model::{
	Language, PermissionLevel, ProblemId, SimpleTestResponse, SubmissionId, UserId,
};
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

const DEFAULT_PAGE_SIZE: u32 = 30;

#[derive(serde::Deserialize)]
struct SubmissionsSearch {
	submitter: Option<String>,
	problem: Option<String>,
	problem_id: Option<String>,
}

async fn submissions(
	extract::State(state): extract::State<Arc<State>>,
	Admin(user): Admin,
	pagination: RawPagination,
	extract::Query(search): extract::Query<SubmissionsSearch>,
) -> Result<Response, Response> {
	let search_submitter = search.submitter.filter(|s| !s.is_empty());
	let search_problem = search.problem.filter(|s| !s.is_empty());
	let search_problem_id = search.problem_id.and_then(|s| s.parse::<ProblemId>().ok());
	let any_search =
		search_submitter.is_some() || search_problem.is_some() || search_problem_id.is_some();

	let pagination = pagination.with_default_page_size(DEFAULT_PAGE_SIZE);
	let limit = pagination.limit();
	let offset = pagination.offset();

	let num_submissions = if any_search {
		query_scalar!(r#"select count(*) as "count: i64" from submissions where (?1 is null or submissions.submitter in (select id from users where instr(display_name, ?1) > 0)) and (?2 is null or submissions.for_problem in (select id from problems where instr(name, ?2) > 0)) and (?3 is null or submissions.for_problem = ?3)"#, search_submitter, search_problem, search_problem_id)
	} else {
		query_scalar!(r#"select count(*) as "count: i64" from submissions"#)
	}
		.fetch_one(&state.database)
		.await
		.map_err(error::internal(Some(&user)))?;

	let submissions = query!(r#"select submissions.id as submission_id, problems.id as problem_id, problems.name as problem_name, users.id as submitter_id, users.display_name as submitter_name, language as "language: Language", submission_time as "submission_time: Timestamp", result as "result: SimpleTestResponse" from submissions inner join problems on submissions.for_problem = problems.id inner join users on submissions.submitter = users.id where (?1 is null or submissions.submitter in (select id from users where instr(display_name, ?1) > 0)) and (?2 is null or submissions.for_problem in (select id from problems where instr(name, ?2) > 0)) and (?3 is null or submissions.for_problem = ?3) order by submissions.id desc limit ?4 offset ?5"#, search_submitter, search_problem, search_problem_id, limit, offset).fetch_all(&state.database).await.map_err(error::internal(Some(&user)))?;

	let body = html! {
		details open[any_search] {
			summary { "Search" }
			form method="get" {
				label { "Submitter name (display name)" input type="text" name="submitter" value=[search_submitter.as_deref()]; }
				label { "Problem name" input type="text" name="problem" value=[search_problem.as_deref()]; }
				label { "Problem ID" input type="number" name="problem_id" value=[search_problem_id]; }
				div.row {
					input type="submit" value="Search";
					a href="/submissions" { "Stop searching" }
				}
			}
		}
		table {
			thead { tr {
				th { "ID" }
				th { "Problem" }
				th { "Submitter" }
				th { "Language" }
				th { "Time" }
				th { "Result" }
			} }
			tbody { @for submission in &submissions { tr {
				td { (submission.submission_id) }
				td { a href={"/problem/"(submission.problem_id)} { (submission.problem_name) } }
				td { a href={"/users/"(submission.submitter_id)} { (submission.submitter_name) } }
				td { (submission.language.name()) }
				td { (submission.submission_time) }
				td { a href={"/submission/"(submission.submission_id)} { (submission.result.map_or("Not yet judged", SimpleTestResponse::as_str)) } }
			} } }
		}
		@if submissions.is_empty() { p { "Nothing here..." } }
		(pagination.make_pager(num_submissions))
	};

	Ok(page("Submissions", Some(&user), &body).into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(handler))
		.route("/delete", post(delete))
		.route("/rejudge", post(rejudge));
	let s_router = axum::Router::new().route("/", get(submissions));
	axum::Router::new()
		.nest("/submission/:id", router)
		.nest("/submissions", s_router)
}
