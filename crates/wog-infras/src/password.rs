use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

use crate::errors::DatabaseError;

/// Hash a plaintext password for storage using Argon2.
pub fn hash_password(password: &str) -> Result<String, DatabaseError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| DatabaseError::Others(format!("Failed to hash password: {e}")))
}

/// Verify a plaintext password against a stored Argon2 hash.
///
/// Returns `false` (rather than an error) for a malformed hash so callers
/// can treat any verification failure as "invalid credentials".
pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed_hash) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok(),
        Err(_) => false,
    }
}
