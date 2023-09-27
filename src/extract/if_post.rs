use axum::async_trait;
use axum::extract::{FromRequest, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{Method, Request};

#[derive(Debug)]
pub struct IfPost<T>(pub Option<T>);

#[async_trait]
impl<S: Sync, T: FromRequestParts<S>> FromRequestParts<S> for IfPost<T> {
	type Rejection = <T as FromRequestParts<S>>::Rejection;

	async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
		match parts.method {
			Method::POST => Ok(Self(Some(T::from_request_parts(parts, state).await?))),
			_ => Ok(Self(None)),
		}
	}
}

#[async_trait]
impl<S: Sync, B: Send + 'static, T: FromRequest<S, B>> FromRequest<S, B> for IfPost<T> {
	type Rejection = <T as FromRequest<S, B>>::Rejection;

	async fn from_request(request: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
		let method = request.method();
		match method {
			&Method::POST => Ok(Self(Some(T::from_request(request, state).await?))),
			_ => Ok(Self(None)),
		}
	}
}
