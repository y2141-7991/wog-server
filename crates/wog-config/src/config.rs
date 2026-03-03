use envconfig::Envconfig;

#[derive(Debug, Envconfig, Clone)]
pub struct AppConfig {
    #[envconfig(from = "DATABASE_URL")]
    pub database_url: String,
    #[envconfig(from = "SERVER_HOST")]
    pub server_host: String,
    #[envconfig(from = "SERVER_PORT")]
    pub server_port: u16,
}
