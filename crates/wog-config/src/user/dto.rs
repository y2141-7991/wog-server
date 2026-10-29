use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use wog_infras::models::{User, UserProfile};


#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Email address or username.
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub is_online: bool,
    pub email_verified: bool,
    pub role: String,
    pub must_change_password: bool,
}

pub struct UserProfileResponse {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub is_online: bool,
}

impl From<User> for UserResponse {
    fn from(value: User) -> Self {
        Self {
            id: value.id,
            email: value.email,
            username: value.username,
            avatar_url: value.avatar_url,
            is_online: value.is_online,
            email_verified: value.email_verified,
            role: value.role,
            must_change_password: value.must_change_password,
        }
    }
}

impl From<UserProfile> for UserResponse {
    fn from(value: UserProfile) -> Self {
        Self {
            id: value.id,
            email: value.email,
            username: value.username,
            avatar_url: value.avatar_url,
            is_online: value.is_online,
            // UserProfile doesn't carry verification state; assume unverified
            // rather than asserting something we don't actually know.
            email_verified: false,
            role: "user".into(),
            must_change_password: false,
        }
    }
}
