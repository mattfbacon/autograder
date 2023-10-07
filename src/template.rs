use axum::response::{IntoResponse, Response};
use maud::{html, Markup, PreEscaped};

use crate::extract::auth::User;
use crate::model::PermissionLevel;
use crate::time::now;

#[derive(Debug, Clone, Copy)]
pub enum BannerKind {
	Info,
	Error,
}

impl BannerKind {
	fn as_str(self) -> &'static str {
		match self {
			Self::Info => "info",
			Self::Error => "error",
		}
	}

	fn name(self) -> &'static str {
		match self {
			Self::Info => "Info",
			Self::Error => "Error",
		}
	}
}

struct Banner<'a> {
	message: &'a str,
	kind: BannerKind,
}

pub struct Page<'a> {
	title: &'a str,
	add_title: bool,
	user: Option<&'a User>,
	body: &'a Markup,
	banner: Option<Banner<'a>>,
}

const FOOTER: PreEscaped<&str> = PreEscaped(
	r#"<p>Autograder is free and libre open-source software (FLOSS) licensed under the GNU Affero General Public License version 3.0 (AGPLv3). The full text of the license is available at <a href="https://www.gnu.org/licenses/agpl-3.0.en.html" target="_blank">https://www.gnu.org/licenses/agpl-3.0.en.html</a>.</p><p>Under this license you have the right as a user to access the source code. It is available at <a href="https://github.com/mattfbacon/autograder" target="_blank">https://github.com/mattfbacon/autograder</a>.</p>"#,
);

fn navbar(user: Option<&User>) -> Markup {
	html! { nav {
		a href="/" {
			b.if-not-narrow { "Autograder" }
			b.if-narrow { "AG" }
		}
		a href="/problems" { "Problems" }
		a href="/about" { "About" }
		@if user.is_some_and(|user| user.permission_level >= PermissionLevel::Admin) {
			a href="/admin" { "Admin" }
		}
		div.spacer role="presentation" {}
		@if let Some(user) = user {
			a href={"/users/"(user.id)} { (user.display_name) }
			a href="/log-out" { "Log out" }
		} @else {
			a href="/log-in" { "Log in" }
			a.if-not-narrow href="/register" { "Register" }
		}
	} }
}

impl<'a> Page<'a> {
	pub fn with_banner(self, kind: BannerKind, message: &'a str) -> Self {
		Self {
			banner: Some(Banner { message, kind }),
			..self
		}
	}

	pub fn custom_title(self) -> Self {
		Self {
			add_title: false,
			..self
		}
	}

	pub fn to_html(&self) -> Markup {
		html! {
			(maud::DOCTYPE);
			html lang="en" {
				head {
					meta charset="UTF-8";
					meta name="viewport" content="width=device-width,initial-scale=1";
					title { (self.title) " - Autograder" }
					link rel="icon" href="/res/favicon.svg" sizes="any" type="image/svg+xml";
					link rel="icon" href="/res/favicon.png" sizes="48x48" type="image/png";
					link rel="stylesheet" href="/res/default.css";
				}
				body {
					(navbar(self.user))
					@if let Some(banner) = &self.banner {
						header class={"banner banner-" (banner.kind.as_str())} {
							(banner.kind.name()) ": " (banner.message)
						}
					}
					main {
						@if self.add_title {
							h1 { (self.title) }
						}
						(self.body)
					}
					footer {
						p { "It is currently " (now()) "." }
						(FOOTER)
						@let admin_email = &crate::CONFIG.admin_email;
						p { "If you have issues, please report them to the admin at " a href={"mailto:"(admin_email)} { (admin_email) } ". Please report any bugs to the GitHub repository linked above." }
					}
				}
			}
		}
	}
}

pub fn page<'a>(title: &'a str, user: Option<&'a User>, body: &'a Markup) -> Page<'a> {
	Page {
		title,
		add_title: true,
		user,
		body,
		banner: None,
	}
}

impl IntoResponse for Page<'_> {
	fn into_response(self) -> Response {
		self.to_html().into_response()
	}
}
