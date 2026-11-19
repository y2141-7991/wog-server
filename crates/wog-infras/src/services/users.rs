use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AppConfig, Claims, TokenPair,
    errors::DatabaseError,
    models::{RefreshOutcome, SessionInfo, User},
    password::{hash_password, verify_password},
    repos::{DynUserRepository, UserRepository},
};

pub const MIN_PASSWORD_LEN: usize = 8;

/// How long an emailed reset link works.
pub const RESET_TOKEN_TTL_MINUTES: i64 = 30;
/// An account gets at most one new link per this many seconds...
const RESET_COOLDOWN_SECONDS: i64 = 60;
/// ...and at most this many per hour, so the form cannot be used to flood an inbox.
const RESET_MAX_PER_HOUR: i64 = 5;

/// What is stored (and looked up) for a reset token: its SHA-256 as lowercase hex.
pub fn hash_reset_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.trim().as_bytes()))
}

/// 244 random bits from the OS generator (two v4 UUIDs), as 64 hex characters.
fn new_reset_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// A second tab that refreshes with the token the first tab has just spent is a race,
/// not theft, for this many seconds.
pub const REFRESH_GRACE_SECONDS: i64 = 10;
/// The Google sign-in redirect carries a code that is good for this long...
pub const OAUTH_CODE_TTL_SECONDS: i64 = 60;
/// ...and a chat-socket ticket for this long. Both work once.
pub const WS_TICKET_TTL_SECONDS: i64 = 30;
pub const CODE_KIND_OAUTH: &str = "oauth";
pub const CODE_KIND_WS: &str = "ws";

/// What a successful refresh hands back.
pub enum Refreshed {
    /// The usual case: new access token and a new refresh token (set as the cookie).
    Rotated(TokenPair),
    /// A racing second tab: a new access token only; the cookie it needs is the one the
    /// first tab already received.
    Replayed { access_token: String },
}

fn session_ended() -> DatabaseError {
    DatabaseError::Unauthorized("Session expired, please sign in again".into())
}

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
    /// Start a sign-in: record the session and issue its first access/refresh pair.
    pub async fn start_session(
        &self,
        cfg: &AppConfig,
        user: &User,
    ) -> Result<TokenPair, DatabaseError> {
        let (sid, jti) = (Uuid::new_v4(), Uuid::new_v4());
        let pair = cfg
            .issue_tokens(user, sid, jti)
            .map_err(|e| DatabaseError::Others(e.to_string()))?;
        self.user_repo
            .create_session(sid, user.id, jti, cfg.refresh_expires_at())
            .await?;
        Ok(pair)
    }

    /// Exchange a (signature-checked) refresh token for new tokens. The token must be
    /// its session's current one; see `UserRepository::rotate_session` for the rest.
    pub async fn refresh_session(
        &self,
        cfg: &AppConfig,
        claims: &Claims,
    ) -> Result<Refreshed, DatabaseError> {
        let (Some(sid), Some(jti)) = (claims.sid, claims.jti) else {
            // Issued before sessions existed: sign in again.
            return Err(session_ended());
        };
        let new_jti = Uuid::new_v4();
        let outcome = self
            .user_repo
            .rotate_session(
                sid,
                jti,
                new_jti,
                cfg.refresh_expires_at(),
                REFRESH_GRACE_SECONDS,
            )
            .await?;
        let user_id = match outcome {
            RefreshOutcome::Rotated(id) | RefreshOutcome::Replayed(id) if id == claims.sub => id,
            RefreshOutcome::Reused => {
                tracing::warn!(session_id = %sid, user_id = %claims.sub, "Refresh token replayed; session revoked");
                return Err(session_ended());
            }
            _ => return Err(session_ended()),
        };
        let user = self.get_user(user_id).await.map_err(|_| session_ended())?;
        match outcome {
            RefreshOutcome::Rotated(_) => Ok(Refreshed::Rotated(
                cfg.issue_tokens(&user, sid, new_jti)
                    .map_err(|e| DatabaseError::Others(e.to_string()))?,
            )),
            _ => Ok(Refreshed::Replayed {
                access_token: cfg
                    .issue_access_token(&user, sid)
                    .map_err(|e| DatabaseError::Others(e.to_string()))?,
            }),
        }
    }

    /// Logout: end one sign-in.
    pub async fn end_session(&self, sid: Uuid) -> Result<(), DatabaseError> {
        self.user_repo.revoke_session(sid).await
    }

    /// The session, if it is still live (not logged out, reset or expired).
    pub async fn live_session(&self, sid: Uuid) -> Result<Option<SessionInfo>, DatabaseError> {
        self.user_repo.find_live_session(sid).await
    }

    /// Mint a single-use code of `kind` and return it; only its hash is stored.
    pub async fn issue_code(
        &self,
        kind: &str,
        user_id: Uuid,
        session_id: Option<Uuid>,
        ttl_seconds: i64,
    ) -> Result<String, DatabaseError> {
        let code = new_reset_token();
        self.user_repo
            .create_auth_code(
                &hash_reset_token(&code),
                kind,
                user_id,
                session_id,
                Utc::now() + Duration::seconds(ttl_seconds),
            )
            .await?;
        Ok(code)
    }

    /// Spend a code: the user (and session, if it was bound to one) it was issued for.
    pub async fn redeem_code(
        &self,
        kind: &str,
        code: &str,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, DatabaseError> {
        self.user_repo
            .redeem_auth_code(&hash_reset_token(code), kind)
            .await
    }

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
    pub async fn resend_verification(
        &self,
        user_id: Uuid,
    ) -> Result<(User, String), DatabaseError> {
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
    /// Start a password reset for `email`.
    ///
    /// Returns the account and the plain token to put in the emailed link, or
    /// `None` when nothing should be sent: the email is unknown, the account
    /// has no password (it signed in with Google only), or it asked too
    /// recently. The caller answers the same way in every case, so the form
    /// never reveals whether an email is registered.
    pub async fn request_password_reset(
        &self,
        email: &str,
    ) -> Result<Option<(User, String)>, DatabaseError> {
        let email = email.trim();
        if email.is_empty() {
            return Ok(None);
        }
        let Some(user) = self.user_repo.find_by_email(email).await? else {
            return Ok(None);
        };
        if user.password_hash().is_none() {
            return Ok(None);
        }

        let now = Utc::now();
        if self
            .user_repo
            .count_reset_tokens_since(user.id, now - Duration::seconds(RESET_COOLDOWN_SECONDS))
            .await?
            > 0
            || self
                .user_repo
                .count_reset_tokens_since(user.id, now - Duration::hours(1))
                .await?
                >= RESET_MAX_PER_HOUR
        {
            return Ok(None);
        }

        let token = new_reset_token();
        self.user_repo
            .create_reset_token(
                user.id,
                &hash_reset_token(&token),
                now + Duration::minutes(RESET_TOKEN_TTL_MINUTES),
            )
            .await?;
        Ok(Some((user, token)))
    }

    /// Finish a reset: spend the token, set the new password and void every
    /// session issued before now.
    pub async fn reset_password(&self, token: &str, new: &str) -> Result<User, DatabaseError> {
        if new.chars().count() < MIN_PASSWORD_LEN {
            return Err(DatabaseError::ValidationError(format!(
                "Password must be at least {MIN_PASSWORD_LEN} characters"
            )));
        }
        if token.trim().is_empty() {
            return Err(DatabaseError::InvalidResetToken);
        }
        self.user_repo
            .reset_password_with_token(&hash_reset_token(token), &hash_password(new)?)
            .await?
            .ok_or(DatabaseError::InvalidResetToken)
    }
}
