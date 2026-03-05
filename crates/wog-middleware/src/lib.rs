use wog_infras::{services::users::UserServices};
use wog_oauth::OAuthServices;

#[derive(Clone)]
pub struct AppState {
    pub user_services: UserServices,
}
