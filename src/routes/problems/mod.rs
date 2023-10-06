use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;
use sqlx::{query, query_scalar};

use crate::extract::auth::User;
use crate::extract::pagination::RawPagination;
use crate::model::PermissionLevel;
use crate::template::page;
use crate::util::s;
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

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	pagination: RawPagination,
	user: Option<User>,
) -> Result<Response, Response> {
	let pagination = pagination.with_default_page_size(DEFAULT_PAGE_SIZE);
	let limit = pagination.limit();
	let offset = pagination.offset();

	let show_invisible = user
		.as_ref()
		.is_some_and(|user| user.permission_level >= PermissionLevel::Admin);

	let num_problems = if show_invisible {
		query_scalar!(r#"select count(*) as "count: i64" from problems"#)
	} else {
		query_scalar!(r#"select count(*) as "count: i64" from problems where visible = 1"#)
	}
	.fetch_one(&state.database)
	.await
	.map_err(error::internal(user.as_ref()))?;

	let problems = query!(
		r#"select id as "id!", name, (select count(*) from submissions where for_problem = problems.id) as "num_submissions!: i64", visible as "visible: bool", (select count(*) from submissions where for_problem = problems.id and result like 'o%') as "num_correct_submissions!: i64" from problems where ? or visible = 1 order by problems.id limit ? offset ?"#,
		show_invisible,
		limit,
		offset,
	).fetch_all(&state.database).await.map_err(error::internal(user.as_ref()))?;

	let body = html! {
		@if user.as_ref().is_some_and(|user| user.permission_level >= PermissionLevel::ProblemAuthor) {
			a href="/problems/new" { "Create a new problem" }
		}
		table {
			thead { tr {
				th { "#" }
				th { "Title" }
				th { "# Submissions" }
				th { "Pass Rate" }
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
				@if show_invisible {
					td { input type="checkbox" role="presentation" title=(if problem.visible { "Visible" } else { "Not visible" }) checked[problem.visible] disabled; }
				}
			} } }
		}
		@if problems.is_empty() { p { "Nothing here..." } }
		(pagination.make_pager(num_problems))
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
