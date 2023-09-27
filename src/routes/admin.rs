use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;
use sqlx::{query, query_scalar};

use crate::extract::auth::Admin;
use crate::extract::pagination::RawPagination;
use crate::model::{PermissionLevel, SimpleTestResponse};
use crate::template::page;
use crate::time::Timestamp;
use crate::{error, State};

const DEFAULT_PAGE_SIZE: u32 = 30;

async fn submissions(
	extract::State(state): extract::State<Arc<State>>,
	Admin(user): Admin,
	pagination: RawPagination,
) -> Result<Response, Response> {
	let pagination = pagination.with_default_page_size(DEFAULT_PAGE_SIZE);
	let limit = pagination.limit();
	let offset = pagination.offset();

	let num_submissions = query_scalar!(r#"select count(*) as "count: i64" from submissions"#)
		.fetch_one(&state.database)
		.await
		.map_err(error::internal(Some(&user)))?;

	let submissions = query!(r#"select submissions.id as submission_id, problems.id as problem_id, problems.name as problem_name, users.id as submitter_id, users.display_name as submitter_name, submission_time as "submission_time: Timestamp", result as "result: SimpleTestResponse" from submissions inner join problems on submissions.for_problem = problems.id inner join users on submissions.submitter = users.id order by submissions.id desc limit ? offset ?"#, limit, offset).fetch_all(&state.database).await.map_err(error::internal(Some(&user)))?;

	let body = html! {
		h1 { "Submissions" }
		table {
			thead { tr {
				th { "ID" }
				th { "Problem" }
				th { "Submitter" }
				th { "Time" }
				th { "Result" }
			} }
			tbody { @for submission in &submissions { tr {
				td { (submission.submission_id) }
				td { a href={"/problems/"(submission.problem_id)} { (submission.problem_name) } }
				td { a href={"/users/"(submission.submitter_id)} { (submission.submitter_name) } }
				td { (submission.submission_time) }
				td { a href={"/submission/"(submission.submission_id)} { (submission.result.as_str()) } }
			} } }
		}
		(pagination.make_pager(num_submissions))
	};

	Ok(page("Submission List", Some(&user), &body).into_response())
}

async fn users(
	extract::State(state): extract::State<Arc<State>>,
	Admin(user): Admin,
	pagination: RawPagination,
) -> Result<Response, Response> {
	let pagination = pagination.with_default_page_size(DEFAULT_PAGE_SIZE);
	let limit = pagination.limit();
	let offset = pagination.offset();

	let num_users = query_scalar!(r#"select count(*) as "count: i64" from users"#)
		.fetch_one(&state.database)
		.await
		.map_err(error::internal(Some(&user)))?;

	let users =
		query!(r#"select id, username, display_name, creation_time as "creation_time: Timestamp", permission_level as "permission_level: PermissionLevel" from users order by id desc limit ? offset ?"#, limit, offset)
			.fetch_all(&state.database)
			.await
			.map_err(error::internal(Some(&user)))?;

	let body = html! {
		h1 { "Users" }
		table {
			thead { tr {
				th { "ID" }
				th { "Username" }
				th { "Display name" }
				th { "Creation time" }
				th { "Permission level" }
			} }
			tbody { @for user in &users { tr {
				td { (user.id) }
				td { a href={"/users/"(user.id)} { (user.username) } }
				td { (user.display_name) }
				td { (user.creation_time) }
				td { (user.permission_level.name()) }
			} } }
		}
		(pagination.make_pager(num_users))
	};

	Ok(page("User List", Some(&user), &body).into_response())
}

async fn admin(Admin(user): Admin) -> Response {
	let body = html! {
		p { a href="/admin/submissions" { "View all submissions" } }
		p { a href="/admin/users" { "View all users" } }
	};

	page("Admin", Some(&user), &body).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(admin))
		.route("/submissions", get(submissions))
		.route("/users", get(users));
	axum::Router::new().nest("/admin", router)
}
