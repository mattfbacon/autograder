use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;
use sqlx::{query, query_scalar};

use crate::extract::auth::User;
use crate::extract::pagination::RawPagination;
use crate::model::{PermissionLevel, UserId};
use crate::template::page;
use crate::util::{s, search_query};
use crate::{error, State};

mod new;
mod problem;

const DEFAULT_PAGE_SIZE: u32 = 20;

/// Returns a pre-multiplied percentage with one decimal place.
#[allow(clippy::cast_precision_loss /* The value is limited to 0.0..=1000.0. */)]
fn pass_rate(num_submissions: i64, num_correct_submissions: i64) -> Option<f32> {
	(num_correct_submissions * 1000)
		.checked_div(num_submissions)
		.map(|v| v as f32 / 10.0)
}

search_query! { struct SearchParameters {
	name: String,
	created_by: String,
	created_by_id: UserId,
	solved_by: String,
	solved_by_id: UserId,
} }

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	pagination: RawPagination,
	extract::Query(search): extract::Query<SearchParameters>,
) -> Result<Response, Response> {
	let pagination = pagination.with_default_page_size(DEFAULT_PAGE_SIZE);
	let limit = pagination.limit();
	let offset = pagination.offset();

	let show_invisible = user
		.as_ref()
		.is_some_and(|user| user.permission_level >= PermissionLevel::Admin);
	let any_search = search.any_set();

	let num_problems = if show_invisible && !any_search {
		query_scalar!(r#"select count(*) as "count: i64" from problems"#)
	} else {
		query_scalar!(r#"select count(*) as "count: i64" from problems inner join users as creator on problems.created_by = creator.id where (?1 or visible = 1) and (?2 is null or instr(problems.name, ?2) > 0) and (?3 is null or instr(creator.display_name, ?3) > 0) and (?4 is null or creator.id = ?4) and (?5 is null or (select 1 from submissions inner join users as submitter on submissions.submitter = submitter.id where for_problem = problems.id and instr(submitter.display_name, ?5) > 0 and result like 'o%') is not null) and (?6 is null or (select 1 from submissions where for_problem = problems.id and submitter = ?6 and result like 'o%') is not null)"#, show_invisible, search.name, search.created_by, search.created_by_id, search.solved_by, search.solved_by_id)
	}
	.fetch_one(&state.database)
	.await
	.map_err(error::sqlx(user.as_ref()))?;

	let user_id = user.as_ref().map(|user| user.id);
	let problems = query!(
		r#"select problems.id as "id!", name, (select count(*) from submissions where for_problem = problems.id) as "num_submissions!: i64", visible as "visible: bool", (select count(*) from submissions where for_problem = problems.id and result like 'o%') as "num_correct_submissions!: i64", (select 1 from submissions where for_problem = problems.id and submitter = ?3 and result like 'o%') is not null as "user_solved!: bool", creator.id as "created_by_id!", creator.display_name as created_by_name from problems inner join users as creator on problems.created_by = creator.id where (?4 or visible = 1) and (?5 is null or instr(problems.name, ?5) > 0) and (?6 is null or instr(creator.display_name, ?6) > 0) and (?7 is null or creator.id = ?7) and (?8 is null or (select 1 from submissions inner join users as submitter on submissions.submitter = submitter.id where for_problem = problems.id and instr(submitter.display_name, ?8) and result like 'o%') is not null) and (?9 is null or (select 1 from submissions where for_problem = problems.id and submitter = ?9 and result like 'o%') is not null) order by problems.id limit ?1 offset ?2"#,
		limit,
		offset,
		user_id,
		show_invisible,
		search.name,
		search.created_by,
		search.created_by_id,
		search.solved_by,
		search.solved_by_id,
	).fetch_all(&state.database).await.map_err(error::sqlx(user.as_ref()))?;

	let body = html! {
		@if user.as_ref().is_some_and(|user| user.permission_level >= PermissionLevel::ProblemAuthor) {
			a href="/problems/new" { "Create a new problem" }
		}
		details open[any_search] {
			summary { "Search" }
			form method="get" {
				label { "Name" input type="text" name="name" value=[search.name.as_deref()]; }
				label { "Creator name (display name)" input type="text" name="created_by" value=[search.created_by.as_deref()]; }
				label { "Creator ID" input type="number" name="created_by_id" value=[search.created_by_id]; }
				label { "Solved by (display name)" input type="text" name="solved_by" value=[search.solved_by.as_deref()]; }
				label { "Solved by ID" input type="number" name="solved_by_id" value=[search.solved_by_id]; }
				div.row {
					input type="submit" value="Search";
					// Intentionally resets pagination, because it's probably not useful.
					a href="/problems" { "Stop searching" }
				}
			}
		}
		table {
			thead { tr {
				th { "#" }
				th { "Title" }
				th { "# Submissions" }
				th { "Pass Rate" }
				th { "Created By" }
				th { "Completed" }
				@if show_invisible {
					th { "Visible" }
				}
			} }
			tbody { @for problem in &problems { tr {
				td { (problem.id) }
				td { a href={ "/problem/" (problem.id) } { (problem.name) } }
				td { (problem.num_submissions) }
				td title={ (problem.num_correct_submissions) " correct submission" (s(problem.num_correct_submissions)) } {
					@if let Some(pass_rate) = pass_rate(problem.num_submissions, problem.num_correct_submissions) {
						(pass_rate) "%"
					} @else {
						"N/A"
					}
				}
				td { a href={"/user/" (problem.created_by_id)} { (problem.created_by_name) } }
				td title={ "You have " (if problem.user_solved { "" } else { "not " }) "completed this problem" } {
					@if problem.user_solved {
						"Yes"
					} @else {
						"No"
					}
				}
				@if show_invisible {
					td { input type="checkbox" role="presentation" title=(if problem.visible { "Visible" } else { "Not visible" }) checked[problem.visible] disabled; }
				}
			} } }
		}
		@if problems.is_empty() { p { "Nothing here..." } }
		(pagination.make_pager(num_problems, search.to_query()))
	};

	Ok(page("Problems", user.as_ref(), &body).into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(handler))
		.merge(new::router());
	axum::Router::new()
		.nest("/problems", router)
		.nest("/problem", problem::router())
}
