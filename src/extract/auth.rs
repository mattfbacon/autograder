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
use sqlx::{query, query_as};
use tower_layer::Layer;
use tower_service::Service;

use crate::error::ErrorResponse;
use crate::extract::return_to;
use crate::model::{PermissionLevel, UserId};
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
		let mut cookie = Cookie::named(COOKIE_NAME);
		cookie.make_removal();
		[("Set-Cookie", cookie.encoded().to_string())]
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
	pub display_name: String,
	pub permission_level: PermissionLevel,
}

impl User {
	fn from_request_parts_sync(parts: &mut Parts) -> Result<Self, Response> {
		if let Some(user) = parts.extensions.get::<User>() {
			Ok(user.clone())
		} else {
			let return_to = return_to::add_to_path("/login", parts.uri.path());
			Err(Redirect::to(&return_to).into_response())
		}
	}
}

async fn extract_user(headers: &HeaderMap, state: &State) -> Option<Result<User, ErrorResponse>> {
	let cookies = headers.get("Cookie")?;
	let cookies = std::str::from_utf8(cookies.as_bytes()).ok()?;

	let token = Cookie::split_parse(cookies)
		.filter_map(Result::ok)
		.find(|cookie| cookie.name() == COOKIE_NAME)?;
	let token = token.value();
	let token: Token = token.parse().ok()?;

	let inner = match query_as!(User, r#"select user as id, users.display_name, users.permission_level as "permission_level!: PermissionLevel" from sessions inner join users on sessions.user = users.id where token = ?"#, token).fetch_optional(&state.database).await {
		Err(error) => return Some(Err(ErrorResponse::internal(error))),
		Ok(inner) => inner?,
	};

	Some(Ok(inner))
}

async fn layer_inner(
	extract::State(state): extract::State<Arc<State>>,
	mut request: Request<Body>,
	next: Next<Body>,
) -> Response {
	match extract_user(request.headers(), &state).await {
		Some(Ok(user)) => {
			request.extensions_mut().insert(user);
		}
		// This is a special case because extracting the user failed, so inherently we cannot have a user present here.
		Some(Err(error)) => return error.into_response(None),
		None => {}
	}
	next.run(request).await
}

#[rustfmt::skip] // Rustfmt chokes on this big generic type.
pub fn layer(
	state: Arc<State>,
) -> impl Layer<
	Route,
	Service = impl Service<Request<Body>, Response = Response, Future = impl Send, Error = Infallible> + Clone,
> + Clone {
	axum::middleware::from_fn_with_state::<_, _, (extract::State<Arc<State>>, Request<Body>)>(
		state,
		layer_inner,
	)
}

#[async_trait]
impl<S: Send> FromRequestParts<S> for User {
	type Rejection = Response;

	async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Response> {
		User::from_request_parts_sync(parts)
	}
}

pub async fn log_in(state: &State, user_id: UserId) -> Result<Token, ErrorResponse> {
	let token = loop {
		let token = Token::generate();
		// If a user logs in when they already have a session, replace the old one.
		let res = query!(
		"insert into sessions (token, user) values (?, ?) on conflict(user) do update set token=excluded.token",
		token,
		user_id,
	).execute(&state.database).await;
		match res {
			Err(sqlx::Error::Database(error))
				if error.kind() == sqlx::error::ErrorKind::UniqueViolation =>
			{
				continue
			}
			Err(error) => return Err(ErrorResponse::internal(error)),
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
