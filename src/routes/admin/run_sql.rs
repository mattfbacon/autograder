use std::pin::pin;
use std::sync::Arc;

use axum::extract;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::StreamExt as _;
use maud::{html, html_into};
use serde::Deserialize;
use sqlx::{query, Column, Either, Row, Type, TypeInfo, ValueRef};

use crate::extract::auth::Admin;
use crate::extract::if_post::IfPost;
use crate::template::page;
use crate::util::{render_debug, s};
use crate::State;

#[derive(Deserialize)]
struct RunSqlForm {
	sql: String,
}

async fn run_sql(
	extract::State(state): extract::State<Arc<State>>,
	Admin(user): Admin,
	IfPost(post): IfPost<extract::Form<RunSqlForm>>,
) -> Result<Response, Response> {
	fn decode<'a, T: sqlx::Decode<'a, sqlx::Sqlite> + Type<sqlx::Sqlite>>(
		raw: sqlx::sqlite::SqliteValueRef<'a>,
	) -> T {
		T::decode(raw).unwrap()
	}

	let results = if let Some(extract::Form(post)) = post {
		let mut buf = String::new();
		let mut results = query(&post.sql).fetch_many(&state.database).peekable();
		let mut results = pin!(results);
		while results.as_mut().peek().await.is_some() {
			let mut first_row = true;

			buf += r#"<section class="query-block">"#;

			while let Some(row) = results.as_mut().peek().await.and_then(|res| {
				if let Ok(Either::Right(row)) = res {
					Some(row)
				} else {
					None
				}
			}) {
				if first_row {
					buf += "<table>";
					html_into! { buf,
						thead { tr {
							@for column in row.columns() {
								th { (render_debug(column.name())) ": " (column.type_info().name()) }
							}
						} }
					};
					buf += "<tbody>";
				}

				html_into! { buf,
					tr {
						@for column in row.columns() { td {
							@let ty = column.type_info();
							@let raw_value = row.try_get_raw(column.ordinal()).unwrap();
							@if raw_value.is_null() {
								"NULL"
							} @else if *ty == <&str>::type_info() {
								(render_debug(decode::<&str>(raw_value)))
							} @else if i64::compatible(ty) {
								(decode::<i64>(raw_value))
							} @else if f64::compatible(ty) {
								(decode::<f64>(raw_value))
							} @else if <&[u8]>::compatible(ty) {
								(hex::encode(decode::<&[u8]>(raw_value)))
							} @else {
								"(could not decode)"
							}
						} }
					}
				}

				first_row = false;
				_ = results.next().await;
			}

			// At least one row was printed.
			if !first_row {
				buf += "</tbody></table>";
			}

			match results.next().await.unwrap() {
				Ok(Either::Left(query_result)) => html_into! { buf,
					@let changes = query_result.rows_affected().try_into().unwrap();
					@let id = query_result.last_insert_rowid();
					p { "Query result: " (changes) " row" (s(changes)) " changed, last insert ID " (id) "." }
				},
				// All rows were already processed above.
				Ok(Either::Right(_row)) => unreachable!(),
				Err(error) => html_into! { buf, p { "Error: " (error) } },
			}

			buf += "</section>";
		}

		Some(buf)
	} else {
		None
	};

	let body = html! {
		@if let Some(results) = results {
			(maud::PreEscaped(results))
			hr;
		}
		p { "Please take care to limit the size of your query results." }
		form method="post" {
			textarea name="sql" cols="40" required {}
			input type="submit" value="Run";
		}
	};

	Ok(page("Run SQL", Some(&user), &body).into_response())
}

pub fn router() -> axum::Router<Arc<State>> {
	axum::Router::new().route("/sql", get(run_sql).post(run_sql))
}
