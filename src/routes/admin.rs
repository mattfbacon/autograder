use std::pin::pin;
use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::StreamExt as _;
use maud::{html, html_into};
use serde::Deserialize;
use sqlx::{query, query_scalar, Column, Either, Row, Type, TypeInfo, ValueRef};

use crate::extract::auth::Admin;
use crate::extract::if_post::IfPost;
use crate::extract::pagination::RawPagination;
use crate::model::{Language, PermissionLevel, ProblemId, SimpleTestResponse};
use crate::template::page;
use crate::time::Timestamp;
use crate::util::{render_debug, s};
use crate::{error, State};

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
					a href="/admin/submissions" { "Stop searching" }
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
		@if users.is_empty() { p { "Nothing here..." } }
		(pagination.make_pager(num_users))
	};

	Ok(page("Users", Some(&user), &body).into_response())
}

#[derive(Deserialize)]
struct RunSqlForm {
	sql: String,
}

async fn run_sql(
	extract::State(state): extract::State<Arc<State>>,
	Admin(user): Admin,
	IfPost(post): IfPost<extract::Form<RunSqlForm>>,
) -> Result<Response, Response> {
	fn decode<'a, T: sqlx::Decode<'a, sqlx::Sqlite> + Type<sqlx::Sqlite>>(
		raw: sqlx::sqlite::SqliteValueRef<'a>,
	) -> T {
		T::decode(raw).unwrap()
	}

	let results = if let Some(extract::Form(post)) = post {
		let mut buf = String::new();
		let mut results = query(&post.sql).fetch_many(&state.database).peekable();
		let mut results = pin!(results);
		while results.as_mut().peek().await.is_some() {
			let mut first_row = true;

			buf += r#"<section class="query-block">"#;

			while let Some(row) = results.as_mut().peek().await.and_then(|res| {
				if let Ok(Either::Right(row)) = res {
					Some(row)
				} else {
					None
				}
			}) {
				if first_row {
					buf += "<table>";
					html_into! { buf,
						thead { tr {
							@for column in row.columns() {
								th { (render_debug(column.name())) ": " (column.type_info().name()) }
							}
						} }
					};
					buf += "<tbody>";
				}

				html_into! { buf,
					tr {
						@for column in row.columns() { td {
							@let ty = column.type_info();
							@let raw_value = row.try_get_raw(column.ordinal()).unwrap();
							@if raw_value.is_null() {
								"NULL"
							} @else if *ty == <&str>::type_info() {
								(render_debug(decode::<&str>(raw_value)))
							} @else if i64::compatible(ty) {
								(decode::<i64>(raw_value))
							} @else if f64::compatible(ty) {
								(decode::<f64>(raw_value))
							} @else if <&[u8]>::compatible(ty) {
								(hex::encode(decode::<&[u8]>(raw_value)))
							} @else {
								"(could not decode)"
							}
						} }
					}
				}

				first_row = false;
				_ = results.next().await;
			}

			// At least one row was printed.
			if !first_row {
				buf += "</tbody></table>";
			}

			match results.next().await.unwrap() {
				Ok(Either::Left(query_result)) => html_into! { buf,
					@let changes = query_result.rows_affected().try_into().unwrap();
					@let id = query_result.last_insert_rowid();
					p { "Query result: " (changes) " row" (s(changes)) " changed, last insert ID " (id) "." }
				},
				// All rows were already processed above.
				Ok(Either::Right(_row)) => unreachable!(),
				Err(error) => html_into! { buf, p { "Error: " (error) } },
			}

			buf += "</section>";
		}

		Some(buf)
	} else {
		None
	};

	let body = html! {
		@if let Some(results) = results {
			(maud::PreEscaped(results))
			hr;
		}
		p { "Please take care to limit the size of your query results." }
		form method="post" {
			textarea name="sql" cols="40" required {}
			input type="submit" value="Run";
		}
	};

	Ok(page("Run SQL", Some(&user), &body).into_response())
}

async fn admin(Admin(user): Admin) -> Response {
	let body = html! {
		p { a href="/admin/submissions" { "View all submissions" } }
		p { a href="/admin/users" { "View all users" } }
		p { a href="/admin/sql" { "Run SQL" } }
	};

	page("Admin", Some(&user), &body).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(admin))
		.route("/submissions", get(submissions))
		.route("/users", get(users))
		.route("/sql", get(run_sql).post(run_sql));
	axum::Router::new().nest("/admin", router)
}
