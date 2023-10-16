use std::fmt::Debug;

pub fn encode_query(raw: &[u8]) -> percent_encoding::PercentEncode<'_> {
	const SET: percent_encoding::AsciiSet = percent_encoding::CONTROLS
		.add(b';')
		.add(b'/')
		.add(b'?')
		.add(b'@')
		.add(b'&')
		.add(b'=')
		.add(b'+')
		.add(b'$')
		.add(b',');

	percent_encoding::percent_encode(raw, &SET)
}

pub fn deserialize_textarea<'de, D: serde::Deserializer<'de>>(de: D) -> Result<String, D::Error> {
	struct Visitor;

	impl<'de> serde::de::Visitor<'de> for Visitor {
		type Value = String;

		fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			formatter.write_str("a string")
		}

		fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
		where
			E: serde::de::Error,
		{
			let mut ret = v.trim().replace("\r\n", "\n");
			if !ret.is_empty() && !ret.ends_with('\n') {
				ret.push('\n');
			}
			Ok(ret)
		}

		// TODO maybe implement `visit_string` to process in-place.
	}

	de.deserialize_str(Visitor)
}

macro_rules! sqlx_type_via {
	($name:ty as $ty:ty) => {
		$crate::util::sqlx_type_via!($name as $ty, (encode, decode));
	};
	($name:ty as $ty:ty, ()) => {
		impl sqlx::Type<sqlx::Sqlite> for $name {
			fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
				<$ty as sqlx::Type<sqlx::Sqlite>>::type_info()
			}

			fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
				<$ty as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
			}
		}
	};

	($name:ty as $ty:ty, (encode $($rest:tt)*)) => {
		impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for $name {
			fn encode_by_ref(
				&self,
				buf: &mut <sqlx::Sqlite as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
			) -> sqlx::encode::IsNull {
				<$ty as sqlx::Encode<'q, sqlx::Sqlite>>::encode(self.repr(), buf)
			}
		}
		$crate::util::sqlx_type_via!($name as $ty, ($($rest)*));
	};

	($name:ty as $ty:ty, (decode $($rest:tt)*)) => {
		impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for $name {
			fn decode(
				value: <sqlx::Sqlite as sqlx::database::HasValueRef<'r>>::ValueRef,
			) -> Result<Self, sqlx::error::BoxDynError> {
				let raw = <$ty as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
				Ok(raw.try_into()?)
			}
		}
		$crate::util::sqlx_type_via!($name as $ty, ($($rest)*));
	};

	($name:ty as $ty:ty, (, $($rest:tt)*)) => {
		$crate::util::sqlx_type_via!($name as $ty, ($($rest)*));
	};
}
pub(crate) use sqlx_type_via;

macro_rules! db_enum {
	($(#[$meta:meta])* $vis:vis enum $name:ident {
		$(
			$(#[$item_meta:meta])*
			$item:ident = $value:expr,
		)*
	}) => {
		#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
		#[serde(try_from = "i64", into = "i64")]
		$(#[$meta])* $vis enum $name {
			$(
				$(#[$item_meta])*
				$item = $value,
			)*
		}

		impl $name {
			$vis const ALL: &[Self] = &[$(Self::$item,)*];

			$vis fn from_repr(repr: i64) -> Option<Self> {
				Some(match repr {
					$(
						$value => Self::$item,
					)*
					_ => return None,
				})
			}

			$vis fn repr(self) -> i64 {
				self as i64
			}
		}

		$crate::util::sqlx_type_via!($name as i64);

		impl From<$name> for i64 {
			fn from(v: $name) -> i64 {
				v.repr()
			}
		}
		impl From<&$name> for i64 {
			fn from(v: &$name) -> i64 {
				(*v).repr()
			}
		}

		impl TryFrom<i64> for $name {
			type Error = String;

			fn try_from(raw: i64) -> Result<Self, Self::Error> {
				Self::from_repr(raw).ok_or_else(|| format!("{}i64 is not recognized as a {}", raw, stringify!($name)))
			}
		}
	};
}
pub(crate) use db_enum;

macro_rules! enum_to_ty {
	($enum:ty, $ty:ty, $from_enum:ident, $to_enum:ident, match { $($enum_v:ident => $ty_v:expr,)* }) => {
		fn $from_enum(v: $enum) -> $ty {
			match v {
				$(<$enum>::$enum_v => $ty_v,)*
			}
		}

		fn $to_enum(v: $ty) -> Option<$enum> {
			Some(match v {
				$($ty_v => <$enum>::$enum_v,)*
				_ => return None,
			})
		}
	};
}
pub(crate) use enum_to_ty;

pub fn s(v: i64) -> &'static str {
	if v == 1 {
		""
	} else {
		"s"
	}
}

pub trait DivCeilPolyfill {
	fn div_ceil_p(self, rhs: Self) -> Self;
}

macro_rules! impl_div_ceil {
	($($ty:ty),* $(,)?) => { $(
		impl DivCeilPolyfill for $ty {
			fn div_ceil_p(self, rhs: Self) -> Self {
				let quotient = self / rhs;
				let remainder = self % rhs;
				if (remainder > 0 && rhs > 0) || (remainder < 0 && rhs < 0) {
					quotient + 1
				} else {
					quotient
				}
			}
		}
	)* };
}

impl_div_ceil!(i64);

pub fn display_fn<F: Fn(&mut std::fmt::Formatter<'_>) -> std::fmt::Result>(
	f: F,
) -> impl std::fmt::Display {
	struct Helper<F>(F);

	impl<F: Fn(&mut std::fmt::Formatter<'_>) -> std::fmt::Result> std::fmt::Display for Helper<F> {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			(self.0)(f)
		}
	}

	Helper(f)
}

pub fn render_debug(value: impl Debug) -> impl maud::Render {
	struct Helper<T>(T);

	impl<T: Debug> maud::Render for Helper<T> {
		fn render_to(&self, w: &mut String) {
			format_args!("{0:?}", self.0).render_to(w);
		}
	}

	Helper(value)
}

pub fn deserialize_non_empty<'de, T: std::str::FromStr, D: serde::Deserializer<'de>>(
	de: D,
) -> Result<Option<T>, D::Error>
where
	<T as std::str::FromStr>::Err: std::fmt::Display,
{
	let raw = <String as serde::Deserialize<'de>>::deserialize(de)?;
	if raw.is_empty() {
		Ok(None)
	} else {
		// TODO This is not optimal because it will copy data for some `T` such as `String`.
		// Using `T: Deserialize<'de>` and `T::deserialize(StringDeserializer::new(raw))` doesn't work because it won't lie to types that expect things like integers, unlike the query param deserializer. Reimplementing that lying logic would be far too complicated.
		raw.parse().map_err(serde::de::Error::custom).map(Some)
	}
}

macro_rules! search_query {
	($vis:vis struct $struct_name:ident { $($name:ident: $ty:ty,)* }) => {
		#[derive(serde::Deserialize)]
		$vis struct $struct_name {
			$(
				#[serde(default)]
				#[serde(deserialize_with = "crate::util::deserialize_non_empty")]
				$name: Option<$ty>,
			)*
		}

		impl $struct_name {
			fn any_set(&self) -> bool {
				$(self.$name.is_some())||*
			}

			fn to_query(&self) -> impl std::fmt::Display + '_ {
				struct Helper<'a>(&'a $struct_name);

				impl std::fmt::Display for Helper<'_> {
					fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
						$(if let Some($name) = &self.0.$name {
							write!(formatter, concat!("&", stringify!($name), "={}"), $crate::util::encode_query($name.to_string().as_bytes()))?;
						})*
						Ok(())
					}
				}

				Helper(self)
			}
		}
	};
}
pub(crate) use search_query;
