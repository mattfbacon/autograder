use std::fmt::{self, Debug, Formatter};
use std::io::Write as _;

use crate::util::{db_enum, enum_to_ty, sqlx_type_via};

pub type Id = i64;

pub type UserId = Id;

pub type ProblemId = Id;

pub type SubmissionId = Id;

db_enum! {
#[derive(Default, PartialOrd, Ord)]
pub enum PermissionLevel {
	#[default]
	User = 0,
	ProblemAuthor = 10,
	Admin = 20,
}
}

impl PermissionLevel {
	pub fn name(self) -> &'static str {
		match self {
			Self::User => "User",
			Self::ProblemAuthor => "Problem author",
			Self::Admin => "Admin",
		}
	}
}

db_enum! {
pub enum Language {
	Python3 = 0,
	C = 1,
	Cpp = 2,
	Java = 3,
	Rust = 4,
}
}

impl Language {
	pub fn name(self) -> &'static str {
		match self {
			Self::Python3 => "Python 3",
			Self::C => "C",
			Self::Cpp => "C++",
			Self::Java => "Java",
			Self::Rust => "Rust",
		}
	}
}

pub struct Tests {
	inner: String,
}

const TEST_CASE_SEPARATOR: &str = "\n===\n";
const TEST_IN_OUT_SEPARATOR: &str = "\n--\n";

fn parse_tests(raw: &str) -> impl Iterator<Item = Option<(&str, &str)>> {
	raw
		.split(TEST_CASE_SEPARATOR)
		.map(|case| case.split_once(TEST_IN_OUT_SEPARATOR))
}

sqlx_type_via!(Tests as String);

impl Tests {
	fn repr(&self) -> String {
		self.inner.clone()
	}

	pub fn cases(&self) -> impl Iterator<Item = (&str, &str)> {
		// We checked that they're valid when the type was constructed.
		parse_tests(&self.inner).map(Option::unwrap)
	}

	pub fn validate(raw: &str) -> Result<(), TestsFromStrError> {
		let mut tests = parse_tests(raw)
			.enumerate()
			.map(|(index, test)| test.ok_or(TestsFromStrError::InvalidTest { index }));
		tests.next().ok_or(TestsFromStrError::NoTests)??;
		tests.try_for_each(|res| res.map(|_| ()))?;
		Ok(())
	}
}

impl Debug for Tests {
	fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
		struct Helper<'a>(&'a str);

		impl Debug for Helper<'_> {
			fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
				f.debug_list()
					.entries(parse_tests(self.0).map(Option::unwrap))
					.finish()
			}
		}

		formatter
			.debug_struct("Tests")
			.field("cases", &Helper(&self.inner))
			.finish()
	}
}

// These error messages are capitalized and include a period,
// in conflict with the standard convention, because they will be shown to the user.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum TestsFromStrError {
	#[error("There are no tests.")]
	NoTests,
	#[error("Test {} is missing the separator {TEST_IN_OUT_SEPARATOR:?} between input and output.", index + 1)]
	InvalidTest { index: usize },
}

impl TryFrom<String> for Tests {
	type Error = TestsFromStrError;

	fn try_from(inner: String) -> Result<Self, Self::Error> {
		Self::validate(&inner)?;
		Ok(Self { inner })
	}
}

sqlx_type_via!(crate::sandbox::TestResponse as String);
enum_to_ty!(crate::sandbox::CaseResultKind, char, case_result_to_char, case_result_from_char, match {
	Correct => 'c',
	Wrong => 'w',
	RuntimeError => 'r',
	TimeLimitExceeded => 't',
	MemoryLimitExceeded => 'm',
});

impl crate::sandbox::TestResponse {
	fn repr(&self) -> String {
		match self {
			Self::Ok(cases) => {
				let mut buf = Vec::with_capacity(1 + cases.len() * 8);

				buf.push(b'?');

				let mut all_correct = true;
				for case in cases {
					all_correct &= matches!(case.kind, crate::sandbox::CaseResultKind::Correct);
					write!(
						buf,
						"{},{},{};",
						case_result_to_char(case.kind),
						case.memory_usage,
						case.time,
					)
					.unwrap();
				}

				buf[0] = if all_correct { b'o' } else { b'e' };

				String::from_utf8(buf).unwrap()
			}
			Self::InvalidProgram(reason) => ["i", reason].concat(),
		}
	}
}

#[derive(Debug, thiserror::Error)]
#[error("invalid serialized test response {0:?}")]
pub struct TestResponseFromStrError(Box<str>);

impl std::str::FromStr for crate::sandbox::TestResponse {
	type Err = TestResponseFromStrError;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		use crate::sandbox::TestResponse as TR;

		fn inner(value: &str) -> Option<TR> {
			let mut chars = value.chars();
			Some(match chars.next()? {
				'o' | 'e' => {
					let rest = chars.as_str();
					let cases = rest
						.split_terminator(';')
						.map(|case| {
							let (kind, rest) = case.split_once(',')?;
							let (memory_usage, time) = rest.split_once(',')?;

							let kind = {
								let mut chars = kind.chars();
								let first = chars.next()?;
								if chars.next().is_some() {
									return None;
								}
								case_result_from_char(first)?
							};
							let memory_usage = memory_usage.parse().ok()?;
							let time = time.parse().ok()?;
							Some(crate::sandbox::CaseResult {
								kind,
								memory_usage,
								time,
							})
						})
						.collect::<Option<Vec<_>>>()?;
					TR::Ok(cases)
				}
				'i' => TR::InvalidProgram(chars.as_str().into()),
				_ => return None,
			})
		}

		inner(value).ok_or_else(|| TestResponseFromStrError(value.into()))
	}
}

impl TryFrom<String> for crate::sandbox::TestResponse {
	type Error = TestResponseFromStrError;

	fn try_from(value: String) -> Result<Self, Self::Error> {
		value.parse()
	}
}

#[derive(Debug, Clone, Copy)]
pub enum SimpleTestResponse {
	Correct,
	Wrong,
	InvalidProgram,
}

impl SimpleTestResponse {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Correct => "Correct",
			Self::Wrong => "Wrong",
			Self::InvalidProgram => "Invalid program",
		}
	}
}

sqlx_type_via!(SimpleTestResponse as String, (decode));

impl TryFrom<String> for SimpleTestResponse {
	type Error = TestResponseFromStrError;

	fn try_from(value: String) -> Result<Self, Self::Error> {
		fn inner(value: &str) -> Option<SimpleTestResponse> {
			Some(match value.chars().next()? {
				'o' => SimpleTestResponse::Correct,
				'e' => SimpleTestResponse::Wrong,
				'i' => SimpleTestResponse::InvalidProgram,
				_ => return None,
			})
		}

		inner(&value).ok_or_else(|| TestResponseFromStrError(value.into()))
	}
}
