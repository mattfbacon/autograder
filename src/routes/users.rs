use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::html;
use serde::Deserialize;
use sqlx::query;

use crate::extract::auth::User;
use crate::model::{PermissionLevel, UserId};
use crate::template::{page, BannerKind};
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

async fn main_page(
	state: &State,
	login_user: Option<&User>,
	req_user_id: UserId,
	action_message: Option<&str>,
) -> Result<Response, Response> {
	let Some(req_user) = query!(r#"select username, display_name, email, creation_time as "creation_time!: Timestamp", permission_level as "permission_level: PermissionLevel", (select count(*) from submissions where submitter = users.id) as "total_submissions!: i64", (select count(distinct for_problem) from submissions where submitter = users.id and result like 'o%') as "solved_problems!: i64" from users where id = ?"#, req_user_id).fetch_optional(&state.database).await.map_err(error::internal(login_user))? else {
		return Err(error::not_found(login_user).await);
	};

	let permission_level = permission_level(login_user, req_user_id);

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
				label { "New display name" input type="text" name="display_name" required value=(req_user.display_name); }
				input type="submit" value="Rename";
			}
			h2 { "Change email" }
			form method="post" action={"/users/"(req_user_id)"/email"} {
				label { "New email (empty for no email)" input type="text" name="email" value=[req_user.email]; }
				input type="submit" value="Change";
			}
			h2 { "Change password" }
			form method="post" action={"/users/"(req_user_id)"/password"} {
				input type="password" autocomplete="new-password" name="password" required;
				input type="submit" value="Change";
			}
			@if permission_level >= UserEditPermissionLevel::Admin {
				h2 { "Change permission level" }
				@if login_user.is_some_and(|login_user| login_user.id == req_user_id) {
					p { "Be careful changing your own access, or you may lock yourself out." }
				}
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
	let mut page = page(&title, login_user, &body);
	if let Some(action_message) = action_message {
		page = page.with_banner(BannerKind::Info, action_message);
	}
	Ok(page.custom_title().into_response())
}

async fn delete(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
) -> Result<Response, Response> {
	if permission_level(login_user.as_ref(), req_user_id) < UserEditPermissionLevel::Edit {
		return Err(error::fake_not_found(login_user.as_ref()).await);
	}

	query!("delete from users where id = ?", req_user_id,)
		.execute(&state.database)
		.await
		.map_err(error::internal(login_user.as_ref()))?;

	Ok(Redirect::to("/").into_response())
}

#[derive(Deserialize)]
struct ChangeEmailForm {
	email: Option<String>,
}

// TODO consider using a macro for these fairly repetitive update functions.

async fn change_email(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
	extract::Form(post): extract::Form<ChangeEmailForm>,
) -> Result<Response, Response> {
	if permission_level(login_user.as_ref(), req_user_id) < UserEditPermissionLevel::Admin {
		return Err(error::fake_not_found(login_user.as_ref()).await);
	}

	let email = post.email.filter(|email| !email.is_empty());

	query!(
		"update users set email = ? where id = ?",
		email,
		req_user_id,
	)
	.execute(&state.database)
	.await
	.map_err(error::internal(login_user.as_ref()))?;

	main_page(
		&state,
		login_user.as_ref(),
		req_user_id,
		Some("Email updated."),
	)
	.await
}

#[derive(Deserialize)]
struct ChangePasswordForm {
	password: String,
}

async fn change_password(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
	extract::Form(post): extract::Form<ChangePasswordForm>,
) -> Result<Response, Response> {
	if permission_level(login_user.as_ref(), req_user_id) < UserEditPermissionLevel::Edit {
		return Err(error::fake_not_found(login_user.as_ref()).await);
	}

	let password = crate::password::hash(&post.password);
	query!(
		"update users set password = ? where id = ?",
		password,
		req_user_id,
	)
	.execute(&state.database)
	.await
	.map_err(error::internal(login_user.as_ref()))?;

	main_page(
		&state,
		login_user.as_ref(),
		req_user_id,
		Some("Password updated."),
	)
	.await
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
		return Err(error::fake_not_found(login_user.as_ref()).await);
	}

	query!(
		"update users set permission_level = ? where id = ?",
		post.permission_level,
		req_user_id,
	)
	.execute(&state.database)
	.await
	.map_err(error::internal(login_user.as_ref()))?;

	main_page(
		&state,
		login_user.as_ref(),
		req_user_id,
		Some("Permission level updated."),
	)
	.await
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
		return Err(error::fake_not_found(login_user.as_ref()).await);
	}

	query!(
		"update users set display_name = ? where id = ?",
		post.display_name,
		req_user_id,
	)
	.execute(&state.database)
	.await
	.map_err(error::internal(login_user.as_ref()))?;

	main_page(
		&state,
		login_user.as_ref(),
		req_user_id,
		Some("Display name updated."),
	)
	.await
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Path(req_user_id): extract::Path<UserId>,
) -> Result<Response, Response> {
	main_page(&state, login_user.as_ref(), req_user_id, None).await
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(handler))
		.route("/delete", post(delete).get(handler))
		.route("/email", post(change_email).get(handler))
		.route("/password", post(change_password).get(handler))
		.route("/permission", post(change_permission).get(handler))
		.route("/rename", post(rename).get(handler));
	axum::Router::new().nest("/users/:id", router)
}
