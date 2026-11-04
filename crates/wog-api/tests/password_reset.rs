//! Forgot / reset password over the real HTTP routes, each test against its
//! own freshly migrated database from `#[sqlx::test]` (so `DATABASE_URL` must
//! point at a Postgres where the user may create databases).
//!
//! The emailed token is never stored in the clear, so the tests plant a token
//! of their own (its hash goes into the table, exactly as the service does).

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;
use wog_api::{api_routes, routers::oauth_routes};
use wog_infras::{
    AppConfig, DefaultConfig,
    crawler::Crawler,
    email::EmailService,
    errors::DatabaseError,
    repos::{
        OAuthRepository, admin::PgAdminRepo, billing::PgBillingRepo,
        marketplace::PgMarketplaceRepo, oauth::OAuthConnection, support::PgSupportRepo,
        users::PgUserRepo,
    },
    services::{
        admin::AdminServices,
        billing::BillingServices,
        marketplace::MarketplaceServices,
        oauth::OAuthServices,
        support::SupportServices,
        users::{UserServices, hash_reset_token},
    },
    support::{HubConfig, LocalHub},
};
use wog_middleware::AppState;

struct NoOAuth;

#[async_trait]
impl OAuthRepository for NoOAuth {
    async fn oauth_auth_url(&self) -> Result<String, DatabaseError> {
        Err(DatabaseError::Others("oauth is off in tests".into()))
    }
    async fn exchange_code(&self, _: String, _: String) -> Result<OAuthConnection, DatabaseError> {
        Err(DatabaseError::Others("oauth is off in tests".into()))
    }
    async fn find_by_oauth(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<wog_infras::models::User>, DatabaseError> {
        Ok(None)
    }
    async fn create_oauth_user(
        &self,
        _: Uuid,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<wog_infras::models::User, DatabaseError> {
        Err(DatabaseError::Others("oauth is off in tests".into()))
    }
}

fn test_config() -> DefaultConfig {
    DefaultConfig {
        database_url: String::new(),
        server_host: "127.0.0.1".into(),
        server_port: 0,
        jwt_secret: "test-jwt-secret".into(),
        refresh_token_secret: "test-refresh-secret".into(),
        access_token_expire_minutes: 60,
        refresh_token_expire_minutes: 600,
        client_url: "https://app.test".into(),
        rest_api_url: "https://app.test".into(),
        smtp_host: String::new(),
        smtp_port: 587,
        smtp_username: String::new(),
        smtp_password: String::new(),
        smtp_from_email: String::new(),
        smtp_from_name: "WoG".into(),
        email_verification_expire_hours: 24,
        upstream_url: "http://127.0.0.1:1".into(),
        upstream_secret: String::new(),
        sepay_webhook_api_key: String::new(),
        sepay_api_token: String::new(),
        sepay_bank_name: String::new(),
        sepay_account_number: String::new(),
        sepay_account_holder: String::new(),
        sepay_payment_code_prefix: "WOG".into(),
        sepay_qr_base_url: String::new(),
        sepay_api_base: String::new(),
        order_ttl_minutes: 30,
        allowed_origins: "https://extra.test, http://localhost:5173".into(),
    }
}

struct App {
    app: Router,
    pool: PgPool,
}

fn app(pool: PgPool) -> App {
    let cfg = test_config();
    let marketplace_repo = Arc::new(PgMarketplaceRepo::new(pool.clone()));
    let state = AppState {
        user_services: UserServices::new(Arc::new(PgUserRepo::new(pool.clone()))),
        oauth_services: OAuthServices::new(Arc::new(NoOAuth)),
        app_config: AppConfig {
            pool: pool.clone(),
            default_config: cfg.clone(),
        },
        email_service: EmailService::new(&cfg),
        marketplace_services: MarketplaceServices::new(
            marketplace_repo.clone(),
            Crawler::new("http://127.0.0.1:1", "secret").unwrap(),
        ),
        admin_services: AdminServices::new(
            Arc::new(PgAdminRepo::new(pool.clone())),
            marketplace_repo,
        ),
        billing_services: BillingServices::new(
            Arc::new(PgBillingRepo::new(pool.clone())),
            None,
            None,
        ),
        support_services: SupportServices::new(
            Arc::new(PgSupportRepo::new(pool.clone())),
            Arc::new(LocalHub::new(HubConfig::default())),
        ),
    };
    let (api, _) = OpenApiRouter::<AppState>::new()
        .merge(api_routes())
        .split_for_parts();
    let app = Router::new()
        .merge(oauth_routes())
        .merge(api)
        .with_state(state);
    App { app, pool }
}

impl App {
    async fn call(
        &self,
        method: &str,
        uri: &str,
        headers: &[(&str, String)],
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            req = req.header(*k, v);
        }
        let body = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let res = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call("POST", uri, &[], Some(body)).await
    }

    /// Register through the API; returns (email, access token, refresh token).
    async fn register(&self, password: &str) -> (String, String, String) {
        let name = format!("u{}", &Uuid::new_v4().simple().to_string()[..10]);
        let email = format!("{name}@example.com");
        let (status, body) = self
            .post(
                "/api/v1/auth/register",
                json!({ "username": name, "email": email, "password": password }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        (
            email,
            body["access_token"].as_str().unwrap().to_string(),
            body["refresh_token"].as_str().unwrap().to_string(),
        )
    }

    async fn login(&self, email: &str, password: &str) -> (StatusCode, Value) {
        self.post(
            "/api/v1/auth/login",
            json!({ "email": email, "password": password }),
        )
        .await
    }

    async fn me(&self, access_token: &str) -> StatusCode {
        self.call(
            "GET",
            "/api/v1/auth/me",
            &[("authorization", format!("Bearer {access_token}"))],
            None,
        )
        .await
        .0
    }

    async fn user_id(&self, email: &str) -> Uuid {
        sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// Plant a reset token the way the service stores one.
    async fn plant_token(&self, email: &str, token: &str, minutes_from_now: i64) {
        let user_id = self.user_id(email).await;
        sqlx::query(
            "INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
             VALUES ($1, $2, NOW() + make_interval(mins => $3::int))",
        )
        .bind(user_id)
        .bind(hash_reset_token(token))
        .bind(minutes_from_now as i32)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn token_rows(&self, email: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM password_reset_tokens t JOIN users u ON u.id = t.user_id WHERE u.email = $1",
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn reset(&self, token: &str, new_password: &str) -> (StatusCode, Value) {
        self.post(
            "/api/v1/auth/reset-password",
            json!({ "token": token, "new_password": new_password }),
        )
        .await
    }

    async fn forgot(&self, email: &str) -> (StatusCode, Value) {
        self.post("/api/v1/auth/forgot-password", json!({ "email": email }))
            .await
    }
}

const OLD_PASSWORD: &str = "old-password-1";
const NEW_PASSWORD: &str = "brand-new-pass-2";

#[sqlx::test(migrations = "../../migrations")]
async fn forgot_password_answers_the_same_for_known_and_unknown_emails(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;

    let known = t.forgot(&email).await;
    let unknown = t.forgot("nobody-here@example.com").await;

    assert_eq!(known.0, StatusCode::OK);
    assert_eq!(
        known, unknown,
        "the response must not reveal who has an account"
    );
    assert_eq!(t.token_rows(&email).await, 1);
    let stray: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens")
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(stray, 1, "an unknown email creates nothing");
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_a_hash_of_the_token_is_stored(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    t.forgot(&email).await;

    let hash: String = sqlx::query_scalar("SELECT token_hash FROM password_reset_tokens")
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(hash.len(), 64);
    assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
    let ttl_minutes: f64 = sqlx::query_scalar(
        "SELECT (EXTRACT(EPOCH FROM (expires_at - created_at)) / 60)::float8 FROM password_reset_tokens",
    )
    .fetch_one(&t.pool)
    .await
    .unwrap();
    assert!(
        (29.0..=31.0).contains(&ttl_minutes),
        "ttl was {ttl_minutes} min"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn forgot_password_is_throttled_per_account(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;

    for _ in 0..3 {
        assert_eq!(t.forgot(&email).await.0, StatusCode::OK);
    }
    assert_eq!(t.token_rows(&email).await, 1, "60 second cooldown");

    // Five links in the last hour (none in the last minute) is the hourly cap.
    sqlx::query(
        "INSERT INTO password_reset_tokens (user_id, token_hash, expires_at, created_at)
         SELECT $1, md5(g::text) || md5(g::text || 'x'), NOW() + interval '30 minutes', NOW() - interval '10 minutes'
         FROM generate_series(1, 4) g",
    )
    .bind(t.user_id(&email).await)
    .execute(&t.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE password_reset_tokens SET created_at = NOW() - interval '5 minutes' WHERE created_at > NOW() - interval '1 minute'")
        .execute(&t.pool)
        .await
        .unwrap();
    assert_eq!(t.token_rows(&email).await, 5);
    assert_eq!(t.forgot(&email).await.0, StatusCode::OK);
    assert_eq!(
        t.token_rows(&email).await,
        5,
        "hourly cap reached: no sixth link"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn accounts_without_a_password_get_no_link(pool: PgPool) {
    let t = app(pool);
    sqlx::query("INSERT INTO users (username, email, email_verified) VALUES ('googler', 'g@example.com', TRUE)")
        .execute(&t.pool)
        .await
        .unwrap();
    assert_eq!(t.forgot("g@example.com").await.0, StatusCode::OK);
    assert_eq!(t.token_rows("g@example.com").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn forgot_password_rejects_an_empty_email(pool: PgPool) {
    let t = app(pool);
    assert_eq!(t.forgot("   ").await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reset_changes_the_password(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    t.plant_token(&email, "tok-happy", 30).await;

    let (status, body) = t.reset("tok-happy", NEW_PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        t.login(&email, OLD_PASSWORD).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(t.login(&email, NEW_PASSWORD).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_link_works_only_once(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    t.plant_token(&email, "tok-once", 30).await;

    assert_eq!(t.reset("tok-once", NEW_PASSWORD).await.0, StatusCode::OK);
    let (status, _) = t.reset("tok-once", "another-password-3").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(t.login(&email, NEW_PASSWORD).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_and_unknown_tokens_are_refused(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    t.plant_token(&email, "tok-old", -1).await;

    assert_eq!(
        t.reset("tok-old", NEW_PASSWORD).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.reset("tok-never-issued", NEW_PASSWORD).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.reset("   ", NEW_PASSWORD).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        t.login(&email, OLD_PASSWORD).await.0,
        StatusCode::OK,
        "password untouched"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_weak_password_does_not_burn_the_link(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    t.plant_token(&email, "tok-weak", 30).await;

    let (status, body) = t.reset("tok-weak", "short").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["message"].as_str().unwrap().contains("at least 8"),
        "{body}"
    );

    assert_eq!(t.reset("tok-weak", NEW_PASSWORD).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_reset_signs_the_account_out_everywhere(pool: PgPool) {
    let t = app(pool);
    let (email, access, refresh) = t.register(OLD_PASSWORD).await;
    assert_eq!(t.me(&access).await, StatusCode::OK);
    t.plant_token(&email, "tok-signout", 30).await;

    // Tokens carry whole seconds; let the reset land in a later second.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(t.reset("tok-signout", NEW_PASSWORD).await.0, StatusCode::OK);

    assert_eq!(
        t.me(&access).await,
        StatusCode::UNAUTHORIZED,
        "old access token"
    );
    let (status, _) = t
        .call(
            "POST",
            "/api/v1/auth/refresh",
            &[("cookie", format!("refresh_token={refresh}"))],
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "old refresh token");
    let (status, _) = t
        .call(
            "GET",
            "/api/v1/support/conversation",
            &[("authorization", format!("Bearer {access}"))],
            None,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "old token on another route"
    );

    let (_, login) = t.login(&email, NEW_PASSWORD).await;
    let fresh = login["access_token"].as_str().unwrap();
    assert_eq!(t.me(fresh).await, StatusCode::OK, "a new sign-in works");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sessions_of_accounts_that_never_reset_are_untouched(pool: PgPool) {
    let t = app(pool);
    let (_, access, refresh) = t.register(OLD_PASSWORD).await;
    let (status, _) = t
        .call(
            "POST",
            "/api/v1/auth/refresh",
            &[("cookie", format!("refresh_token={refresh}"))],
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(t.me(&access).await, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn using_one_link_cancels_the_others(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    t.plant_token(&email, "tok-a", 30).await;
    t.plant_token(&email, "tok-b", 30).await;

    assert_eq!(t.reset("tok-a", NEW_PASSWORD).await.0, StatusCode::OK);
    assert_eq!(
        t.reset("tok-b", "yet-another-pass-4").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(t.login(&email, NEW_PASSWORD).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_reset_clears_the_forced_password_change(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    sqlx::query("UPDATE users SET must_change_password = TRUE WHERE email = $1")
        .bind(&email)
        .execute(&t.pool)
        .await
        .unwrap();
    t.plant_token(&email, "tok-forced", 30).await;

    assert_eq!(t.reset("tok-forced", NEW_PASSWORD).await.0, StatusCode::OK);
    let (_, login) = t.login(&email, NEW_PASSWORD).await;
    assert_eq!(login["user"]["must_change_password"], json!(false));
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_simultaneous_resets_with_one_link_cannot_both_win(pool: PgPool) {
    let t = Arc::new(app(pool));
    let (email, _, _) = t.register(OLD_PASSWORD).await;
    t.plant_token(&email, "tok-race", 30).await;

    let a = tokio::spawn({
        let t = t.clone();
        async move { t.reset("tok-race", "race-password-aaa").await.0 }
    });
    let b = tokio::spawn({
        let t = t.clone();
        async move { t.reset("tok-race", "race-password-bbb").await.0 }
    });
    let mut results = vec![a.await.unwrap(), b.await.unwrap()];
    results.sort();
    assert_eq!(results, vec![StatusCode::OK, StatusCode::BAD_REQUEST]);
}
