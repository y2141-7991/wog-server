use std::time::Duration;

use anyhow::{Context, Result};

use envconfig::Envconfig;
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope,
    TokenResponse, TokenUrl,
    basic::{BasicClient, BasicTokenResponse},
};
use oauth2_reqwest::ReqwestClient;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum OAuthServiceError {
    #[error("Provider api error: {0}")]
    ProviderApi(String),
    #[error("User email not verified")]
    UserEmailNotVerified,
    #[error("User info operation not supported")]
    UserInfoNotSupported,
    #[error("Can not parse result payload")]
    ParseError(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ResponseError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl From<ResponseError> for OAuthServiceError {
    fn from(value: ResponseError) -> Self {
        match value {
            other => OAuthServiceError::ParseError(other.to_string()),
        }
    }
}

impl From<anyhow::Error> for OAuthServiceError {
    fn from(value: anyhow::Error) -> Self {
        match value {
            other => OAuthServiceError::ProviderApi(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<Duration>,
}

impl From<BasicTokenResponse> for OAuthTokens {
    fn from(response: BasicTokenResponse) -> Self {
        OAuthTokens {
            access_token: String::from(response.access_token().secret().to_owned()),
            refresh_token: response.refresh_token().map(|t| t.secret().to_owned()),
            expires_in: response.expires_in(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum OAuthProvider {
    Google,
}

pub struct OAuthUser {
    email: String,
    name: String,
    sub: String,
    exp: i64,
}

#[derive(Deserialize)]
pub struct GoogleUser {
    pub sub: String,
    pub email: String,
    pub email_verified: Option<bool>,
    pub picture: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Envconfig)]
pub struct EnvConf {
    #[envconfig(from = "GOOGLE_CLIENT_ID")]
    google_client_id: String,
    #[envconfig(from = "GOOGLE_CLIENT_SECRET")]
    google_client_secret: String,
    #[envconfig(from = "CLIENT_URL")]
    client_url: String,
}

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    provider: OAuthProvider,
    client_id: String,
    client_secret: String,
    auth_url: String,
    token_url: String,
    redirect_url: String,
    scopes: Vec<String>,
    user_info_url: Option<String>,
}

#[derive(Debug)]
pub struct AuthorizeUrl {
    pub url: String,
    pub crsf_token: String,
    pub pkce_verifier: String,
}

impl OAuthConfig {
    pub fn new(provider: &str, env_conf: EnvConf) -> Result<Self> {
        match provider {
            "google" => Ok(Self {
                provider: OAuthProvider::Google,
                client_id: env_conf.google_client_id,
                client_secret: env_conf.google_client_secret,
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                redirect_url: format!("{}/api/v1/auth/oauth/google/callback", env_conf.client_url),
                scopes: vec!["email".to_string(), "openid".to_string()],
                user_info_url: Some("https://www.googleapis.com/oauth2/v3/userinfo".to_string()),
            }),
            _ => Err(anyhow::anyhow!(format!("Invalid provider {}", provider))),
        }
    }
}

pub fn http_client() -> Client {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Client should build")
}

#[derive(Debug, Clone)]
pub struct OAuthService {
    pub oauth_config: OAuthConfig,
    pub http_client: Client,
    pub oauth_client: OAuthBasicClient,
}

pub type OAuthBasicClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

impl OAuthService {
    pub fn new(provider: &str) -> Self {
        let env_var = EnvConf::init_from_env().expect("Env var not found");
        let config = OAuthConfig::new(provider, env_var).expect("Env var not found");
        let client = Self::build_basic_client(&config);
        Self {
            oauth_config: config,
            http_client: http_client(),
            oauth_client: client,
        }
    }
    pub fn build_basic_client(oauth_config: &OAuthConfig) -> OAuthBasicClient {
        let client_id = ClientId::new(oauth_config.client_id.to_owned());
        let client_secret = ClientSecret::new(oauth_config.client_secret.to_owned());
        let auth_url =
            AuthUrl::new(oauth_config.auth_url.to_owned()).expect("Invalid authorization URL");
        let token_url =
            TokenUrl::new(oauth_config.token_url.to_owned()).expect("Invalid token URL");
        let redirect_url =
            RedirectUrl::new(oauth_config.redirect_url.to_string()).expect("Invalid redirect URL");
        BasicClient::new(client_id)
            .set_client_secret(client_secret)
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(redirect_url)
            .set_auth_type(AuthType::RequestBody)
    }
    pub fn authenticate_url(&self) -> AuthorizeUrl {
        let client = &self.oauth_client;
        let (pkce_code_challenge, pkce_code_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut client = client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(pkce_code_challenge);
        for scope in self.oauth_config.scopes.clone() {
            client = client.add_scope(Scope::new(scope));
        }
        let (auth_url, _csrf_token) = client.add_extra_param("access_type", "offline").url();
        AuthorizeUrl {
            url: auth_url.to_string(),
            crsf_token: String::from(_csrf_token.secret().to_owned()),
            pkce_verifier: String::from(pkce_code_verifier.secret().to_owned()),
        }
    }

    pub async fn fetch_user_info(
        &self,
        access_token: String,
    ) -> Result<GoogleUser, OAuthServiceError> {
        let url = self
            .oauth_config
            .user_info_url
            .as_ref()
            .ok_or(OAuthServiceError::UserInfoNotSupported)?;
        let res = self
            .http_client
            .get(url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", access_token),
            )
            .send()
            .await
            .map_err(|e| {
                OAuthServiceError::ProviderApi(format!("Failed to fetch Google user: {}", e))
            })?;
        let oauth_user = res.json::<GoogleUser>().await.map_err(|e| {
            OAuthServiceError::ParseError(format!("Failed to parse Google user: {}", e))
        })?;

        Ok(oauth_user)
    }

    pub async fn exchange_code(
        &self,
        auth_code: String,
        pkce_code_verifier: String,
    ) -> Result<OAuthTokens, OAuthServiceError> {
        let client = &self.oauth_client;
        let standard_token_response = client
            .exchange_code(AuthorizationCode::new(auth_code.to_owned()))
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_code_verifier.to_owned()))
            .request_async(&ReqwestClient::from(self.http_client.clone()))
            .await
            .map_err(|e| OAuthServiceError::ProviderApi(format!("Token exchange failed: {}", e)))?;
        Ok(OAuthTokens::from(standard_token_response))
    }

    pub async fn exchange_refresh_token(
        &self,
        refresh_token: String,
    ) -> Result<OAuthTokens, OAuthServiceError> {
        let client = &self.oauth_client;
        let standard_token_response = client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_owned()))
            .request_async(&ReqwestClient::from(self.http_client.clone()))
            .await
            .map_err(|e| OAuthServiceError::ProviderApi(format!("Refresh token exchange failed: {}", e)))?;
        Ok(OAuthTokens::from(standard_token_response))
    }
}
