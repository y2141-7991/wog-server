use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::User,
    password::{hash_password, verify_password},
    repos::{DynUserRepository, UserRepository},
};

pub const MIN_PASSWORD_LEN: usize = 8;

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

    /// Verify a credential and return the matching user. `identifier` is an
    /// email address or a username.
    ///
    /// Returns `DatabaseError::InvalidCredentials` both when the account is
    /// unknown and when the password doesn't match, so callers never leak
    /// whether an email is registered. The same error is returned for an
    /// OAuth-only account (no password set).
    pub async fn login(&self, identifier: &str, password: &str) -> Result<User, DatabaseError> {
        let identifier = identifier.trim();
        let user = match self.user_repo.find_by_email(identifier).await? {
            Some(user) => user,
            None => self
                .user_repo
                .find_by_username(identifier)
                .await?
                .ok_or(DatabaseError::InvalidCredentials)?,
        };

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

    /// Replace a user's password after checking the current one. Also clears
    /// the forced-change flag set on a bootstrapped admin.
    pub async fn change_password(
        &self,
        user_id: Uuid,
        current: &str,
        new: &str,
    ) -> Result<User, DatabaseError> {
        let user = self.get_user(user_id).await?;
        let stored_hash = user.password_hash().ok_or_else(|| {
            DatabaseError::ValidationError("This account has no password to change".into())
        })?;
        if !verify_password(current, stored_hash) {
            return Err(DatabaseError::ValidationError(
                "Current password is incorrect".into(),
            ));
        }
        if new.len() < MIN_PASSWORD_LEN {
            return Err(DatabaseError::ValidationError(format!(
                "New password must be at least {MIN_PASSWORD_LEN} characters"
            )));
        }
        if new == current {
            return Err(DatabaseError::ValidationError(
                "New password must differ from the current one".into(),
            ));
        }
        self.user_repo
            .update_password(user_id, &hash_password(new)?)
            .await
    }
}
