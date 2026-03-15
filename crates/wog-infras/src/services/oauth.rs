use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::User,
    repos::{DynOAuthRepository, OAuthRepository, oauth::OAuthConnection},
};

#[derive(Clone)]
pub struct OAuthServices {
    oauth_repo: DynOAuthRepository,
}

impl OAuthServices {
    pub fn new(oauth_repo: std::sync::Arc<dyn OAuthRepository + Send + Sync>) -> Self {
        Self { oauth_repo }
    }
}

impl OAuthServices {
    pub async fn auth_url(&self) -> Result<String, DatabaseError> {
        info!("Generating OAuth authorization URL");
        let url = self.oauth_repo.oauth_auth_url().await?;
        debug!(url = %url, "OAuth authorization URL generated");
        Ok(url)
    }
    async fn exchange_code(
        &self,
        code: String,
        csrf: String,
    ) -> Result<OAuthConnection, DatabaseError> {
        info!("Exchanging OAuth authorization code");
        let result = self.oauth_repo.exchange_code(code, csrf).await?;
        info!(email = %result.user.email, "OAuth code exchange successful");
        Ok(result)
    }
    async fn find_by_oauth(
        &self,
        provider: &str,
        sub: &str,
    ) -> Result<Option<User>, DatabaseError> {
        debug!(provider = %provider, sub = %sub, "Looking up existing OAuth user");
        let user = self.oauth_repo.find_by_oauth(provider, sub).await?;
        match &user {
            Some(u) => info!(user_id = %u.id, "Found existing OAuth user"),
            None => info!(provider = %provider, "No existing user found, will create new"),
        }
        Ok(user)
    }
    async fn create_oauth_user(
        &self,
        id: uuid::Uuid,
        email: &str,
        username: &str,
        avatar_url: &str,
        provider: &str,
        sub: &str,
    ) -> Result<User, DatabaseError> {
        info!(email = %email, username = %username, provider = %provider, "Creating new OAuth user");
        self.oauth_repo
            .create_oauth_user(id, email, username, avatar_url, provider, sub)
            .await
            .map_err(|e| {
                warn!(email = %email, error = %e, "Failed to create OAuth user");
                DatabaseError::ExistedDataError(
                    format!("Username or email already exists: {}", e).into(),
                )
            })
    }
    pub async fn callback(&self, code: String, csrf: String) -> Result<User, DatabaseError> {
        info!("OAuth callback started");
        let oauth = self.exchange_code(code, csrf).await?;

        let user = match self.find_by_oauth("google", &oauth.user.sub).await? {
            Some(existing) => existing,
            _ => {
                let google_user = oauth.user;
                let username = google_user.name.unwrap_or_else(|| {
                    google_user
                        .email
                        .split('@')
                        .next()
                        .unwrap_or("user")
                        .to_string()
                });

                self.create_oauth_user(
                    Uuid::new_v4(),
                    &google_user.email,
                    &username,
                    google_user.picture.as_deref().unwrap_or(""),
                    "google",
                    &google_user.sub,
                )
                .await?
            }
        };
        info!(user_id = %user.id, "OAuth callback completed successfully");
        Ok(user)
    }
}
