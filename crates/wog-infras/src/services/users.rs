use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::User,
    password::{hash_password, verify_password},
    repos::{DynUserRepository, UserRepository},
};

#[derive(Clone)]
pub struct UserServices {
    user_repo: DynUserRepository,
}

impl UserServices {
    pub fn new(user_repo: std::sync::Arc<dyn UserRepository + Send + Sync>) -> Self {
        Self { user_repo }
    }
}

impl UserServices {
    pub async fn get_user(&self, id: Uuid) -> Result<User, DatabaseError> {
        self.user_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("User not found".into()))
    }

    /// Create a new account with an email/password credential and issue an
    /// email-verification token for it. The account is usable immediately;
    /// `email_verified` starts `false` until the link in that email is opened.
    pub async fn register(
        &self,
        username: &str,
        email: &str,
        password: &str,
    ) -> Result<(User, String), DatabaseError> {
        let hashed = hash_password(password)?;
        let user = self
            .user_repo
            .create(Uuid::new_v4(), username, email, &hashed)
            .await?;
        let token = self.issue_verification_token(user.id).await?;
        Ok((user, token))
    }

    /// Verify an email/password credential and return the matching user.
    ///
    /// Returns `DatabaseError::InvalidCredentials` both when the email is
    /// unknown and when the password doesn't match, so callers never leak
    /// whether an email is registered. The same error is returned for an
    /// OAuth-only account (no password set).
    pub async fn login(&self, email: &str, password: &str) -> Result<User, DatabaseError> {
        let user = self
            .user_repo
            .find_by_email(email)
            .await?
            .ok_or(DatabaseError::InvalidCredentials)?;

        let stored_hash = user
            .password_hash()
            .ok_or(DatabaseError::InvalidCredentials)?;

        if !verify_password(password, stored_hash) {
            return Err(DatabaseError::InvalidCredentials);
        }

        Ok(user)
    }

    /// Generate and store a fresh 24-hour email-verification token for a user.
    async fn issue_verification_token(&self, user_id: Uuid) -> Result<String, DatabaseError> {
        let token = Uuid::new_v4().to_string();
        let expires_at = Utc::now() + Duration::hours(24);
        self.user_repo
            .create_verification_token(user_id, &token, expires_at)
            .await?;
        Ok(token)
    }

    /// Consume a verification token and mark its owning account as verified.
    pub async fn verify_email(&self, token: &str) -> Result<User, DatabaseError> {
        let record = self
            .user_repo
            .find_verification_token(token)
            .await?
            .ok_or(DatabaseError::InvalidVerificationToken)?;

        if record.expires_at < Utc::now() {
            self.user_repo.delete_verification_token(token).await?;
            return Err(DatabaseError::InvalidVerificationToken);
        }

        let user = self.user_repo.mark_email_verified(record.user_id).await?;
        self.user_repo.delete_verification_token(token).await?;
        Ok(user)
    }

    /// Issue a new verification token for an already-authenticated, not-yet-verified user.
    pub async fn resend_verification(&self, user_id: Uuid) -> Result<(User, String), DatabaseError> {
        let user = self.get_user(user_id).await?;
        if user.email_verified {
            return Err(DatabaseError::ValidationError(
                "Email is already verified".into(),
            ));
        }
        let token = self.issue_verification_token(user_id).await?;
        Ok((user, token))
    }
}
