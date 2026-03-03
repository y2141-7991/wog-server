use serde::Serialize;
use uuid::Uuid;
use wog_infras::models::{User, UserProfile};

#[derive(Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub is_online: bool,
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
        }
    }
}
