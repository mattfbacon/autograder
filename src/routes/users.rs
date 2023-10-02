use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::html;
use serde::Deserialize;
use sqlx::query;

use crate::error::ErrorResponse;
use crate::extract::auth::User;
use crate::model::{PermissionLevel, UserId};
use crate::template::page;
use crate::time::Timestamp;
use crate::util::s;
use crate::{error, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum UserEditPermissionLevel {
	None,
	Edit,
	Admin,
}

fn permission_level(login_user: Option<&User>, req_user_id: UserId) -> UserEditPermissionLevel {
	login_user.map_or(UserEditPermissionLevel::None, |login_user| {
		if login_user.permission_level >= PermissionLevel::Admin {
			UserEditPermissionLevel::Admin
		} else if login_user.id == req_user_id {
			UserEditPermissionLevel::Edit
		} else {
			UserEditPermissionLevel::None
		}
	})
}

async fn delete(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
) -> Result<Response, Response> {
	if permission_level(login_user.as_ref(), req_user_id) < UserEditPermissionLevel::Edit {
		return Err(
			ErrorResponse::not_found()
				.await
				.into_response(login_user.as_ref()),
		);
	}

	query!("delete from users where id = ?", req_user_id,)
		.execute(&state.database)
		.await
		.map_err(error::internal(login_user.as_ref()))?;

	Ok(Redirect::to("/").into_response())
}

#[derive(Deserialize)]
struct ChangePermissionForm {
	permission_level: PermissionLevel,
}

async fn change_permission(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
	extract::Form(post): extract::Form<ChangePermissionForm>,
) -> Result<Response, Response> {
	if permission_level(login_user.as_ref(), req_user_id) < UserEditPermissionLevel::Admin {
		return Err(
			ErrorResponse::not_found()
				.await
				.into_response(login_user.as_ref()),
		);
	}

	query!(
		"update users set permission_level = ? where id = ?",
		post.permission_level,
		req_user_id,
	)
	.execute(&state.database)
	.await
	.map_err(error::internal(login_user.as_ref()))?;

	Ok(Redirect::to(&format!("/users/{req_user_id}")).into_response())
}

#[derive(Deserialize)]
struct RenameForm {
	display_name: String,
}

async fn rename(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
	extract::Form(post): extract::Form<RenameForm>,
) -> Result<Response, Response> {
	if permission_level(login_user.as_ref(), req_user_id) < UserEditPermissionLevel::Edit {
		return Err(
			ErrorResponse::not_found()
				.await
				.into_response(login_user.as_ref()),
		);
	}

	query!(
		"update users set display_name = ? where id = ?",
		post.display_name,
		req_user_id,
	)
	.execute(&state.database)
	.await
	.map_err(error::internal(login_user.as_ref()))?;

	Ok(Redirect::to(&format!("/users/{req_user_id}")).into_response())
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
) -> Result<Response, Response> {
	let Some(req_user) = query!(r#"select username, display_name, creation_time as "creation_time!: Timestamp", permission_level as "permission_level: PermissionLevel", (select count(*) from submissions where submitter = users.id) as "total_submissions!: i64", (select count(distinct for_problem) from submissions where submitter = users.id and result like 'o%') as "solved_problems!: i64" from users where id = ?"#, req_user_id).fetch_optional(&state.database).await.map_err(error::internal(login_user.as_ref()))? else {
		return Err(error::not_found(login_user.as_ref()).await);
	};

	let permission_level = permission_level(login_user.as_ref(), req_user_id);

	let body = html! {
		h1 { (req_user.display_name) " (" (req_user.username) ")" }
		p { "Permission level: " (req_user.permission_level.name()) }
		p { "Created at " (req_user.creation_time) "." }
		p { "Has made " (req_user.total_submissions) " submission" (s(req_user.total_submissions)) "." }
		p { "Has solved " (req_user.solved_problems) " problem" (s(req_user.solved_problems)) "." }
		@if permission_level >= UserEditPermissionLevel::Edit {
			hr;
			h2 { "Change display name" }
			form method="post" action={"/users/"(req_user_id)"/rename"} {
				label { "New display name" input type="text" name="display_name" required; }
				input type="submit" value="Rename";
			}
			@if permission_level >= UserEditPermissionLevel::Admin {
				h2 { "Change permission level" }
				form method="post" action={"/users/"(req_user_id)"/permission"} {
					label { "New permission level" select name="permission_level" required {
						@for &level in PermissionLevel::ALL {
							option value=(level.repr()) selected[level == req_user.permission_level] { (level.name()) }
						}
					} }
					input type="submit" value="Change";
				}
			}
			h2 { "Delete" }
			form method="post" action={"/users/"(req_user_id)"/delete"} {
				input type="submit" value="Delete";
			}
		}
	};

	let title = format!("(User) {}", req_user.display_name);
	let page = page(&title, login_user.as_ref(), &body);
	Ok(page.custom_title().into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(handler))
		.route("/rename", post(rename))
		.route("/permission", post(change_permission))
		.route("/delete", post(delete));
	axum::Router::new().nest("/users/:id", router)
}
