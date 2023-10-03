use std::sync::Arc;

use axum::extract;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::html;
use serde::Deserialize;
use sqlx::{query, query_as, query_scalar};

use super::new::problem_form;
use super::pass_rate;
use crate::error::ErrorResponse;
use crate::extract::auth::User;
use crate::extract::if_post::IfPost;
use crate::model::{Language, PermissionLevel, ProblemId, SubmissionId, Tests, UserId};
use crate::template::{page, BannerKind};
use crate::time::{now, Timestamp};
use crate::util::{deserialize_textarea, s};
use crate::{error, State};

fn can_edit(user: Option<&User>, problem_created_by: Option<UserId>) -> bool {
	user.is_some_and(|user| {
		user.permission_level >= PermissionLevel::Admin
			|| problem_created_by.is_some_and(|problem_created_by| {
				user.permission_level >= PermissionLevel::ProblemAuthor && user.id == problem_created_by
			})
	})
}

async fn handle_edit_post(
	state: &State,
	problem_id: ProblemId,
	post: &super::new::Problem,
) -> Result<(), ErrorResponse> {
	Tests::validate(&post.tests).map_err(|error| ErrorResponse::bad_request(error.to_string()))?;

	query!("update problems set name = ?, description = ?, time_limit = ?, memory_limit = ?, visible = ?, tests = ? where id = ?", post.name, post.description, post.time_limit, post.memory_limit, post.visible, post.tests, problem_id).execute(&state.database).await.map_err(ErrorResponse::internal)?;

	Ok(())
}

async fn edit_handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(problem_id): extract::Path<ProblemId>,
	IfPost(post): IfPost<extract::Form<super::new::Problem>>,
) -> Result<Response, Response> {
	let Some(created_by) = query_scalar!("select created_by from problems where id = ?", problem_id)
		.fetch_optional(&state.database)
		.await
		.map_err(error::internal(user.as_ref()))?
	else {
		return Err(error::not_found(user.as_ref()).await);
	};
	if !can_edit(user.as_ref(), created_by) {
		return Err(error::fake_not_found(user.as_ref()).await);
	}

	let post_res = if let Some(post) = post {
		Some(handle_edit_post(&state, problem_id, &post).await)
	} else {
		None
	};

	let Some(problem) = query_as!(
		super::new::Problem,
		r#"select name, description, time_limit as "time_limit: u32", memory_limit as "memory_limit: u32", visible as "visible: bool", tests from problems inner join users on problems.created_by = users.id where problems.id = ?"#,
		problem_id,
	)
	.fetch_optional(&state.database)
	.await
	.map_err(error::internal(user.as_ref()))?
	else {
		return Err(error::not_found(user.as_ref()).await);
	};

	let title = format!("Edit Problem {problem_id}");
	let body = html! {
		p { a href={"/problem/"(problem_id)} { "Back to problem page" } }
		form method="post" {
			(problem_form(Some(&problem)))
			input type="submit" value="Edit";
		}
	};

	let status = post_res
		.as_ref()
		.and_then(|res| res.as_ref().err())
		.map_or(StatusCode::OK, |error| error.status);
	let mut page = page(&title, user.as_ref(), &body);
	page = match &post_res {
		Some(Ok(())) => page.with_banner(BannerKind::Info, "Problem updated"),
		Some(Err(error)) => page.with_banner(BannerKind::Error, &error.message),
		None => page,
	};
	Ok((status, page).into_response())
}

async fn delete_handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Path(problem_id): extract::Path<ProblemId>,
) -> Result<Response, Response> {
	let Some(created_by) = query_scalar!("select created_by from problems where id = ?", problem_id)
		.fetch_optional(&state.database)
		.await
		.map_err(error::internal(user.as_ref()))?
	else {
		return Err(error::not_found(user.as_ref()).await);
	};
	if !can_edit(user.as_ref(), created_by) {
		return Err(error::fake_not_found(user.as_ref()).await);
	}

	query!("delete from problems where id = ?", problem_id)
		.execute(&state.database)
		.await
		.map_err(error::internal(user.as_ref()))?;

	Ok(Redirect::to("/problems").into_response())
}

#[derive(Debug, Deserialize)]
struct Post {
	language: Language,
	#[serde(deserialize_with = "deserialize_textarea")]
	code: String,
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
		r#"select name, description, problems.creation_time as "creation_time: Timestamp", users.id as "created_by_id?", users.display_name as "created_by_name?", (select count(*) from submissions where for_problem = problems.id) as "num_submissions!: i64", (select count(*) from submissions where for_problem = problems.id and result like 'o%') as "num_correct_submissions!: i64", tests as "tests: Tests" from problems left join users on problems.created_by = users.id where problems.id = ?"#,
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
		@if can_edit(user.as_ref(), problem.created_by_id) {
			div.row {
				a href={"/problem/"(problem_id)"/edit"} { "Edit" }
				form method="post" action={"/problem/"(problem_id)"/delete"} { input type="submit" value="Delete"; }
				@if user.as_ref().is_some_and(|user| user.permission_level >= PermissionLevel::Admin) {
					a href={"/admin/submissions?problem_id="(problem_id)} { "View submissions" }
				}
			}
		}
		p { b {
			"Created "
			@if let Some(created_by_id) = problem.created_by_id {
				@if let Some(created_by_name) = &problem.created_by_name {
					"by " a href={"/users/"(created_by_id)} { (created_by_name) } " "
				}
			}
			"on " (problem.creation_time)
			" | "
			(problem.num_submissions) " submission" (s(problem.num_submissions))
			@if let Some(pass_rate) = pass_rate {
				", " (pass_rate) "% correct"
			}
		} }
		p { (problem.description) }
		div.sample-io {
			h2 { "Sample input" }
			pre { code { (sample_input) } }
			h2 { "Sample output" }
			pre { code { (sample_output) } }
		}

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
	let router = axum::Router::new()
		.route("/", get(handler).post(handler))
		.route("/edit", get(edit_handler).post(edit_handler))
		.route("/delete", post(delete_handler));
	axum::Router::new().nest("/:id", router)
}
