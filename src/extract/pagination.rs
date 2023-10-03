use std::ops::Range;

use axum::async_trait;
use axum::extract::{self, FromRequestParts};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use maud::html;

use crate::error::ErrorResponse;
use crate::util::{display_fn, s, DivCeilPolyfill as _};

const VALID_PAGE_SIZE: Range<u32> = 1..101;

#[derive(Debug, Clone, Copy)]
pub struct RawPagination {
	page: u32,
	page_size: Option<u32>,
}

impl RawPagination {
	pub fn with_default_page_size(self, default_page_size: u32) -> Pagination {
		debug_assert!(
			VALID_PAGE_SIZE.contains(&default_page_size),
			"invalid default page size {default_page_size}"
		);

		Pagination {
			page: self.page,
			page_size: self.page_size.unwrap_or(default_page_size),
		}
	}
}

#[derive(Debug, Clone, Copy)]
pub struct Pagination {
	page: u32,
	page_size: u32,
}

impl Pagination {
	pub fn display_page(self) -> i64 {
		i64::from(self.page) + 1
	}

	pub fn prev(self, num_entries: i64) -> Option<Self> {
		if num_entries != 0 && self.offset() >= num_entries {
			return Some(Self {
				page: self
					.num_pages(num_entries)
					.try_into()
					.map_or(u32::MAX, |x: u32| x.saturating_sub(1)),
				page_size: self.page_size,
			});
		}

		self.page.checked_sub(1).map(|page| Self {
			page,
			page_size: self.page_size,
		})
	}

	pub fn next(self, num_entries: i64) -> Option<Self> {
		let next_page = self.page.checked_add(1)?;
		let next = Self {
			page: next_page,
			page_size: self.page_size,
		};
		Some(next).filter(|next| next.offset() < num_entries)
	}

	pub fn num_pages(self, num_entries: i64) -> i64 {
		num_entries.div_ceil_p(self.limit()).max(1)
	}

	pub fn limit(self) -> i64 {
		self.page_size.into()
	}

	pub fn offset(self) -> i64 {
		let page_size: i64 = self.page_size.into();
		let page: i64 = self.page.into();

		page * page_size
	}

	pub fn query(self) -> impl std::fmt::Display {
		display_fn(move |fmt| write!(fmt, "page={}&page_size={}", self.page, self.page_size))
	}

	pub fn make_pager(self, num_entries: i64) -> maud::Markup {
		html! {
			p title={(num_entries) " submission"(s(num_entries)) " total"} {
				@if let Some(prev) = self.prev(num_entries) {
					a href={"?"(prev.query())} title={"Go to page " (prev.display_page())} { "Prev" }
					" "
				}
				"Page " (self.display_page()) " of " (self.num_pages(num_entries))
				@if let Some(next) = self.next(num_entries) {
					" "
					a href={"?"(next.query())} title={"Go to page " (next.display_page())} { "Next" }
				}
			}
		}
	}
}

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for RawPagination {
	type Rejection = Response;

	async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Response> {
		#[derive(serde::Deserialize)]
		struct Inner {
			page: Option<u32>,
			page_size: Option<u32>,
		}

		let extract::Query(inner) =
			<extract::Query<Inner> as FromRequestParts<S>>::from_request_parts(parts, state)
				.await
				.map_err(IntoResponse::into_response)?;
		if inner
			.page_size
			.is_some_and(|page_size| !VALID_PAGE_SIZE.contains(&page_size))
		{
			return Err(
				ErrorResponse::bad_request(format!(
					"Page size is out of range; valid range is {VALID_PAGE_SIZE:?}"
				))
				.into_response_in_extractor(parts),
			);
		}
		Ok(Self {
			page: inner.page.unwrap_or(0),
			page_size: inner.page_size,
		})
	}
}
