use std::sync::Arc;

use axum::extract;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use mail_builder::MessageBuilder;
use mail_send::SmtpClientBuilder;
use maud::html;
use serde::Deserialize;
use sqlx::query;

use crate::error::{self, ErrorResponse};
use crate::extract::auth::User;
use crate::extract::if_post::IfPost;
use crate::model::UserId;
use crate::template::{page, BannerKind};
use crate::time::{minutes, now, Timestamp};
use crate::State;

type Key = i64;

#[derive(Deserialize)]
struct ActionQuery {
	user: UserId,
	key: Key,
}

async fn remove_email(
	extract::State(state): extract::State<Arc<State>>,
	user: Option<User>,
	extract::Query(query): extract::Query<ActionQuery>,
) -> Result<Response, Response> {
	let result = query!(
		"update users set email = null, remove_email_key = random() where id = ? and remove_email_key = ?",
		query.user,
		query.key,
	)
	.execute(&state.database)
	.await
	.map_err(error::internal(user.as_ref()))?;

	if result.rows_affected() > 0 {
		tracing::info!(user=?query.user, "email removed");
	} else {
		tracing::warn!(user=?query.user, req_user=?user, "invalid email removal request");
	}

	let body = html! {
		p { "Sorry for the spam. If these parameters were correct, your email was removed from the user's account." }
	};

	Ok(page("Email Removal", user.as_ref(), &body).into_response())
}

#[derive(Deserialize)]
struct ResetForm {
	password: String,
}

async fn do_reset(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	extract::Query(query): extract::Query<ActionQuery>,
	IfPost(post): IfPost<extract::Form<ResetForm>>,
) -> Result<Response, Response> {
	let Some(req_user) = query!(
		r#"select display_name, password_reset_key, password_reset_expiration as "password_reset_expiration: Timestamp" from users where id = ?"#,
		query.user,
	)
	.fetch_optional(&state.database)
	.await
	.map_err(error::internal(login_user.as_ref()))?
	else {
		return Err(error::not_found(login_user.as_ref()).await);
	};

	if req_user.password_reset_key != Some(query.key)
		|| !req_user
			.password_reset_expiration
			.is_some_and(|expiration| !expiration.is_in_past())
	{
		let error = ErrorResponse::bad_request("Invalid or expired password reset key.");
		return Err(error.into_response(login_user.as_ref()));
	}

	if let Some(post) = post {
		let password = crate::password::hash(&post.password);
		query!(
			"update users set password = ? where id = ?",
			password,
			query.user,
		)
		.execute(&state.database)
		.await
		.map_err(error::internal(login_user.as_ref()))?;

		return Ok(Redirect::to("/log-in").into_response());
	}

	let body = html! {
		p { "Resetting password for " (req_user.display_name) }
		form method="post" {
			label { "New Password" input type="password" autocomplete="new-password" required name="password" placeholder="hunter2"; }
			input type="submit" value="Reset Password";
		}
	};

	Ok(page("Reset Password", login_user.as_ref(), &body).into_response())
}

#[derive(serde::Deserialize, Debug)]
struct Form {
	username: String,
}

async fn handle_post(state: &State, post: Form) -> Result<(), ErrorResponse> {
	let smtp = &crate::CONFIG.smtp;

	let user = query!(
		r#"select id as "id!", display_name, email, remove_email_key, password_reset_expiration as "password_reset_expiration: Timestamp" from users where username = ?"#,
		post.username,
	)
	.fetch_optional(&state.database)
	.await
	.map_err(ErrorResponse::internal)?;

	let Some(user) = user else {
		return Ok(());
	};
	let Some(user_email) = user.email else {
		return Ok(());
	};

	if user
		.password_reset_expiration
		.is_some_and(|expiration| !expiration.is_in_past())
	{
		return Err(ErrorResponse::bad_request(
			"There is already a password reset in progress. Please wait a few minutes and try again.",
		));
	}

	let new_expiration = now() + minutes(5);
	let password_reset_key: Key = rand::random();
	query!(
		"update users set password_reset_expiration = ?, password_reset_key = ? where id = ?",
		new_expiration,
		password_reset_key,
		user.id
	)
	.execute(&state.database)
	.await
	.map_err(ErrorResponse::internal)?;

	let body = format!("\
Someone requested to reset the password for the account with the username {username:?} which is associated with this email {user_email:?}. \
To set your new password, please go to <https://{url}/password-reset/do-reset?user={user_id}&key={password_reset_key}>.

If this was not you, you can ignore this email. If your email should not be associated with this account at all, please go to <https://{url}/password-reset/remove-email?user={user_id}&key={remove_email_key}>.
",
		username = post.username,
		url = crate::CONFIG.external_url,
		user_id = user.id,
		remove_email_key = user.remove_email_key,
	);

	let send_fut = async move {
		let message = MessageBuilder::new()
			.from(("Autograder", smtp.username.as_str()))
			.to((user.display_name.as_str(), user_email.as_str()))
			.subject("AutoGrader Password Recovery")
			.text_body(body);

		SmtpClientBuilder::new(smtp.host.as_str(), smtp.port)
			.helo_host("dummy.faircode.eu")
			.implicit_tls(smtp.implicit_tls)
			.credentials((smtp.username.as_str(), smtp.password.as_str()))
			.connect()
			.await?
			.send(message)
			.await
	};
	tokio::spawn(async move {
		if let Err(error) = send_fut.await {
			tracing::error!(?post, "error sending email: {error}");
		}
	});

	Ok(())
}

async fn handler(
	extract::State(state): extract::State<Arc<State>>,
	login_user: Option<User>,
	IfPost(post): IfPost<extract::Form<Form>>,
) -> Response {
	let res = if let Some(extract::Form(post)) = post {
		Some(handle_post(&state, post).await)
	} else {
		None
	};

	let body = html! {
		p { "This will send an email to the address associated with the account, if the account exists." }
		form method="post" {
			label { "Username" input type="text" name="username" required; }
			input type="submit" value="Send";
		}
	};

	let mut page = page("Reset Password", login_user.as_ref(), &body);
	let status = res
		.as_ref()
		.and_then(|res| res.as_ref().err())
		.map_or(StatusCode::OK, |error| error.status);
	page = match &res {
		Some(Ok(())) => {
			let message = "If everything is in order, an email was sent. If you don't get an email, recheck your parameters, then contact the admin.";
			page.with_banner(BannerKind::Info, message)
		}
		Some(Err(error)) => page.with_banner(BannerKind::Error, &error.message),
		None => page,
	};
	(status, page).into_response()
}

pub fn router() -> axum::Router<Arc<State>> {
	let router = axum::Router::new()
		.route("/", get(handler).post(handler))
		.route("/remove-email", get(remove_email))
		.route("/do-reset", get(do_reset).post(do_reset));
	axum::Router::new().nest("/password-reset", router)
}
