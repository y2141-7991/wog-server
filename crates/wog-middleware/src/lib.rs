use wog_infras::{AppConfig, services::{oauth::OAuthServices, users::UserServices}};

#[derive(Clone)]
pub struct AppState {
    pub user_services: UserServices,
    pub app_config: AppConfig,
    pub oauth_services: OAuthServices
}
