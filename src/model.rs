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

sqlx_type_via!(crate::sandbox::TestResponse as String);
enum_to_ty!(crate::sandbox::CaseResult, char, case_result_to_char, case_result_from_char, match {
	Correct => 'c',
	Wrong => 'w',
	RuntimeError => 'r',
	Timeout => 't',
});

impl crate::sandbox::TestResponse {
	fn repr(&self) -> String {
		match self {
			Self::Ok(cases) => {
				let first = if cases
					.iter()
					.all(|case| matches!(case, crate::sandbox::CaseResult::Correct))
				{
					'o'
				} else {
					'e'
				};
				let cases = cases.iter().copied().map(case_result_to_char);
				std::iter::once(first).chain(cases).collect()
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
					let cases = chars
						.map(case_result_from_char)
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
