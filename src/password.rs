#[derive(sqlx::Type, Debug)]
#[sqlx(transparent)]
pub struct Hash(String);

impl Hash {
	#[must_use]
	pub fn new(password: &str) -> Self {
		Self(bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap())
	}

	pub fn verify(&self, password: &str) -> bcrypt::BcryptResult<bool> {
		bcrypt::verify(password, &self.0)
	}
}

#[must_use]
pub fn hash(password: &str) -> Hash {
	Hash::new(password)
}
