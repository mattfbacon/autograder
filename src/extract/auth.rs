use std::convert::Infallible;
use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, IntoResponseParts, Redirect, Response, ResponseParts};
use axum::routing::Route;
use axum::{async_trait, extract};
use cookie::Cookie;
use sqlx::query;
use tower_layer::Layer;
use tower_service::Service;

use crate::error::ErrorResponse;
use crate::extract::return_to;
use crate::model::{PermissionLevel, UserId};
use crate::time::{days, minutes, now, Duration, Timestamp};
use crate::State;

const COOKIE_NAME: &str = "token";

type TokenData = [u8; 32];

#[derive(Clone, Copy)]
pub struct Token(TokenData);

impl Token {
	fn generate() -> Self {
		Self(rand::random())
	}

	// Don't want to copy around this large array when there's no reason to.
	#[allow(clippy::wrong_self_convention)]
	#[allow(clippy::needless_borrow)]
	pub fn to_cookie(&self) -> Cookie<'static> {
		let encoded = hex::encode(&self.0);
		Cookie::build(COOKIE_NAME, encoded)
			.secure(true)
			.http_only(true)
			.same_site(cookie::SameSite::Strict)
			.permanent()
			.finish()
	}

	pub fn removal() -> impl IntoResponseParts {
		struct Helper;

		impl IntoResponseParts for Helper {
			type Error = Infallible;

			fn into_response_parts(self, mut res: ResponseParts) -> Result<ResponseParts, Infallible> {
				let mut cookie = Cookie::named(COOKIE_NAME);
				cookie.make_removal();
				let value = cookie.encoded().to_string().try_into().unwrap();
				res.headers_mut().append("Set-Cookie", value);
				Ok(res)
			}
		}

		Helper
	}
}

impl IntoResponseParts for &Token {
	type Error = Infallible;

	fn into_response_parts(self, mut parts: ResponseParts) -> Result<ResponseParts, Self::Error> {
		let cookie = self.to_cookie();
		parts.headers_mut().insert(
			"Set-Cookie",
			cookie.encoded().to_string().try_into().unwrap(),
		);
		Ok(parts)
	}
}

impl Debug for Token {
	fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
		struct Helper<'a>(&'a [u8]);

		impl Debug for Helper<'_> {
			fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
				for byte in self.0 {
					write!(formatter, "{byte:02x}")?;
				}
				Ok(())
			}
		}

		formatter
			.debug_tuple("Token")
			.field(&Helper(&self.0))
			.finish()
	}
}

impl FromStr for Token {
	type Err = hex::FromHexError;

	fn from_str(raw: &str) -> Result<Self, Self::Err> {
		let mut buf = TokenData::default();

		hex::decode_to_slice(raw, &mut buf)?;

		Ok(Self(buf))
	}
}

impl sqlx::Type<sqlx::Sqlite> for Token {
	fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
		<&[u8] as sqlx::Type<sqlx::Sqlite>>::type_info()
	}

	fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
		<&[u8] as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
	}
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for Token {
	fn encode_by_ref(
		&self,
		buf: &mut <sqlx::Sqlite as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
	) -> sqlx::encode::IsNull {
		<Vec<u8> as sqlx::Encode<'q, sqlx::Sqlite>>::encode(self.0.into(), buf)
	}
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for Token {
	fn decode(
		value: <sqlx::Sqlite as sqlx::database::HasValueRef<'r>>::ValueRef,
	) -> Result<Self, sqlx::error::BoxDynError> {
		let bytes_slice = <&[u8] as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
		let bytes = bytes_slice.try_into()?;
		Ok(Self(bytes))
	}
}

#[derive(Debug, Clone)]
pub struct User {
	pub id: UserId,
	pub display_name: Arc<str>,
	pub permission_level: PermissionLevel,
}

impl User {
	fn from_request_parts_sync(parts: &mut Parts) -> Result<Self, Response> {
		if let Some(user) = parts.extensions.get::<User>() {
			Ok(user.clone())
		} else {
			let return_to = return_to::add_to_path("/log-in", parts.uri.path());
			Err(Redirect::to(&return_to).into_response())
		}
	}
}

struct NoUser {
	should_remove_token: bool,
}

async fn extract_user(
	headers: &HeaderMap,
	state: &State,
) -> Result<Result<User, NoUser>, ErrorResponse> {
	fn extract_cookie(headers: &HeaderMap) -> Result<Token, NoUser> {
		headers
			.get("Cookie")
			.ok_or(false)
			.and_then(|header| {
				std::str::from_utf8(header.as_bytes())
					.ok()
					.and_then(|header| {
						Cookie::split_parse(header)
							.filter_map(Result::ok)
							.find(|cookie| cookie.name() == COOKIE_NAME)
					})
					.and_then(|cookie| cookie.value().parse().ok())
					.ok_or(true)
			})
			.map_err(|should_remove_token| NoUser {
				should_remove_token,
			})
	}

	let token = match extract_cookie(headers) {
		Ok(token) => token,
		Err(error) => return Ok(Err(error)),
	};

	let Some(inner) = query!(r#"select user as id, users.display_name as "display_name: Arc<str>", users.permission_level as "permission_level!: PermissionLevel", expiration as "expiration: Timestamp" from sessions inner join users on sessions.user = users.id where token = ?"#, token).fetch_optional(&state.database).await.map_err(ErrorResponse::sqlx)? else { return Ok(Err(NoUser { should_remove_token: true })); };

	let now = now();

	if inner.expiration.is_before(now) {
		query!("delete from sessions where token = ?", token)
			.execute(&state.database)
			.await
			.map_err(ErrorResponse::sqlx)?;
		return Ok(Err(NoUser {
			should_remove_token: true,
		}));
	}

	let new_expiration = now + TOKEN_DURATION;
	// Try not to do too many database writes. `last_used` doesn't need that much precision.
	if (inner.expiration - new_expiration).abs() > TOKEN_DURATION_GRANULARITY {
		query!(
			"update sessions set expiration = ? where token = ?",
			new_expiration,
			token,
		)
		.execute(&state.database)
		.await
		.map_err(ErrorResponse::sqlx)?;
	}

	Ok(Ok(User {
		id: inner.id,
		display_name: inner.display_name,
		permission_level: inner.permission_level,
	}))
}

async fn layer_inner(
	extract::State(state): extract::State<Arc<State>>,
	mut request: Request<Body>,
	next: Next<Body>,
) -> Response {
	match extract_user(request.headers(), &state).await {
		Ok(maybe_user) => {
			let should_remove_token = maybe_user
				.as_ref()
				.is_err_and(|error| error.should_remove_token);
			if let Ok(user) = maybe_user {
				request.extensions_mut().insert(user);
			}

			let response = next.run(request).await;

			if should_remove_token {
				(Token::removal(), response).into_response()
			} else {
				response
			}
		}
		// This is a special case because extracting the user failed, so inherently we cannot have a user present here.
		Err(error) => error.into_response(None),
	}
}

#[rustfmt::skip] // Rustfmt chokes on this big generic type.
pub fn layer(
	state: Arc<State>,
) -> impl Layer<
	Route,
	Service = impl Service<Request<Body>, Response = Response, Future = impl Send, Error = Infallible> + Clone,
> + Clone {
	axum::middleware::from_fn_with_state(state, layer_inner)
}

#[async_trait]
impl<S: Send> FromRequestParts<S> for User {
	type Rejection = Response;

	async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Response> {
		User::from_request_parts_sync(parts)
	}
}

pub const TOKEN_DURATION: Duration = days(5);
const TOKEN_DURATION_GRANULARITY: Duration = minutes(5);

pub async fn log_in(state: &State, user_id: UserId) -> Result<Token, ErrorResponse> {
	let token = loop {
		let token = Token::generate();
		let expiration = now() + TOKEN_DURATION;
		let res = query!(
			"insert into sessions (token, user, expiration) values (?, ?, ?)",
			token,
			user_id,
			expiration,
		)
		.execute(&state.database)
		.await;
		match res {
			Err(sqlx::Error::Database(error))
				if error.kind() == sqlx::error::ErrorKind::UniqueViolation =>
			{
				continue;
			}
			Err(error) => return Err(ErrorResponse::sqlx(error)),
			Ok(_) => break token,
		}
	};
	Ok(token)
}

macro_rules! permission_extractor {
	($name:ident) => {
		pub struct $name(pub User);

		#[async_trait]
		impl<S: Send> FromRequestParts<S> for $name {
			type Rejection = Response;

			async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Response> {
				let user = User::from_request_parts_sync(parts)?;
				let required_level = PermissionLevel::$name;
				if user.permission_level >= required_level {
					Ok(Self(user))
				} else {
					tracing::warn!(
						?user,
						?required_level,
						?parts.uri,
						"user tried to access page without required permission level; pretending it doesn't exist",
					);
					Err(crate::error::not_found(Some(&user)).await)
				}
			}
		}
	};
}

permission_extractor!(ProblemAuthor);
permission_extractor!(Admin);
