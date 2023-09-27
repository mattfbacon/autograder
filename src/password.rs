#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct Hash(String);

impl Hash {
	pub fn new(password: &str) -> Self {
		Self(bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap())
	}
}
