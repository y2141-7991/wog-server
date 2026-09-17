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
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub is_online: bool,
    pub email_verified: bool,
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
        }
    }
}
