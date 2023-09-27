use std::time::SystemTime;

#[derive(Debug, Clone, Copy)]
pub struct Timestamp {
	seconds_since_epoch: i64,
}

impl Timestamp {
	#[must_use]
	pub fn now() -> Self {
		let seconds_since_epoch = SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_secs()
			.try_into()
			.unwrap();
		Self {
			seconds_since_epoch,
		}
	}
}

impl std::fmt::Display for Timestamp {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let time = time::OffsetDateTime::from_unix_timestamp(self.seconds_since_epoch).unwrap();
		write!(
			formatter,
			"{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC",
			year = time.year(),
			month = time.month() as u8,
			day = time.day(),
			hour = time.hour(),
			minute = time.minute(),
			second = time.second(),
		)
	}
}

#[must_use]
pub fn now() -> Timestamp {
	Timestamp::now()
}

impl sqlx::Type<sqlx::Sqlite> for Timestamp {
	fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
		<i64 as sqlx::Type<sqlx::Sqlite>>::type_info()
	}

	fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
		<i64 as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
	}
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for Timestamp {
	fn encode_by_ref(
		&self,
		buf: &mut <sqlx::Sqlite as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
	) -> sqlx::encode::IsNull {
		<i64 as sqlx::Encode<'q, sqlx::Sqlite>>::encode(self.seconds_since_epoch, buf)
	}
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for Timestamp {
	fn decode(
		value: <sqlx::Sqlite as sqlx::database::HasValueRef<'r>>::ValueRef,
	) -> Result<Self, sqlx::error::BoxDynError> {
		<i64 as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value).map(|raw| Self {
			seconds_since_epoch: raw,
		})
	}
}
