use wog_config::config::AppConfig;
use wog_infras::services::users::UserServices;

#[derive(Clone)]
pub struct AppState {
    pub app_config: AppConfig,
    pub user_services: UserServices,
}
