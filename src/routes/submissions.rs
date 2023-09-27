use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use maud::html;
use sqlx::query;

use crate::extract::auth::User;
use crate::model::{Language, PermissionLevel, SubmissionId};
use crate::sandbox::TestResponse;
use crate::template::page;
use crate::time::Timestamp;
use crate::{error, State};

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
		h1 { "Submission for " a href={"/problems/"(submission.problem_id)} { "Problem " (submission.problem_id) ": " (submission.problem_name) } }
		p { b { "By " (submission.submitter_name) " | Submitted at " (submission.submission_time) } }
		@match &submission.result {
			TestResponse::Ok(cases) => {
				h2 { "Cases" }
				ol {
					@for case in cases {
						li { (case.as_str()) }
					}
				}
			},
			TestResponse::InvalidProgram(reason) => {
				h2 { "Program was invalid" }
				pre { code { (reason) } }
			},
		}
		h2 { "Code" }
		p { "Language: " (submission.language.name()) }
		pre { code { (submission.code) } }
	};

	Ok(page("Submission", user.as_ref(), &body).into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/submission/:id", get(handler))
}
