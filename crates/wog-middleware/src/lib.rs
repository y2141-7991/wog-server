use wog_config::config::AppConfig;
use wog_infras::services::users::UserServices;
use wog_oauth::OAuthServices;

#[derive(Clone)]
pub struct AppState {
    pub app_config: AppConfig,
    pub user_services: UserServices,
    pub oauth_services: OAuthServices
}
