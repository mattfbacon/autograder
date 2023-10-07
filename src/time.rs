use std::ops::{Add, Sub};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::util::sqlx_type_via;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "i64", into = "i64")]
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

	#[must_use]
	pub fn is_before(self, other: Self) -> bool {
		self < other
	}

	#[must_use]
	pub fn is_in_past(self) -> bool {
		self.is_before(now())
	}

	fn repr(self) -> i64 {
		self.seconds_since_epoch
	}
}

impl From<i64> for Timestamp {
	fn from(seconds_since_epoch: i64) -> Self {
		Self {
			seconds_since_epoch,
		}
	}
}

impl From<Timestamp> for i64 {
	fn from(ts: Timestamp) -> Self {
		ts.seconds_since_epoch
	}
}

sqlx_type_via!(Timestamp as i64);

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

/// In seconds.
pub type Duration = i64;

impl Add<Duration> for Timestamp {
	type Output = Self;

	fn add(self, offset: Duration) -> Self {
		Self {
			seconds_since_epoch: self.seconds_since_epoch + offset,
		}
	}
}

impl Sub<Duration> for Timestamp {
	type Output = Self;

	fn sub(self, offset: Duration) -> Self {
		Self {
			seconds_since_epoch: self.seconds_since_epoch - offset,
		}
	}
}

impl Sub<Self> for Timestamp {
	type Output = Duration;

	fn sub(self, other: Self) -> Duration {
		self.seconds_since_epoch - other.seconds_since_epoch
	}
}

pub const fn minutes(v: i64) -> Duration {
	v * 60
}

pub const fn hours(v: i64) -> Duration {
	minutes(v) * 60
}

pub const fn days(v: i64) -> Duration {
	hours(v) * 24
}
