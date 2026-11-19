//! Sign-in sessions over the real HTTP routes: the Bearer access token, the scoped
//! refresh cookie, rotation, replay detection, logout and the one-time Google code.
//! Each test runs against its own freshly migrated database from `#[sqlx::test]`.
#![allow(dead_code)]

use std::sync::Arc;

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
        cookie_secure: true,
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

    /// Like `call`, but also returns every `Set-Cookie` header.
    async fn raw(
        &self,
        method: &str,
        uri: &str,
        headers: &[(&str, String)],
        body: Option<Value>,
    ) -> (StatusCode, Value, Vec<String>) {
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
        let cookies = res
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            cookies,
        )
    }

    /// POST `/api/v1/auth/refresh` with `refresh` as the cookie.
    async fn refresh(&self, refresh: &str) -> (StatusCode, Value, Vec<String>) {
        self.raw(
            "POST",
            "/api/v1/auth/refresh",
            &[("cookie", format!("refresh_token={refresh}"))],
            None,
        )
        .await
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

/// The refresh token's value out of a response's `Set-Cookie` headers (skipping the
/// expiring legacy ones).
fn refresh_cookie(cookies: &[String]) -> Option<String> {
    cookies
        .iter()
        .find(|c| c.starts_with("refresh_token=") && !c.starts_with("refresh_token=;"))
        .map(|c| {
            c["refresh_token=".len()..]
                .split(';')
                .next()
                .unwrap()
                .to_string()
        })
}

async fn session_count(t: &App, revoked: bool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM refresh_sessions WHERE (revoked_at IS NOT NULL) = $1")
        .bind(revoked)
        .fetch_one(&t.pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn sign_in_sets_one_scoped_refresh_cookie_and_returns_the_access_token(pool: PgPool) {
    let t = app(pool);
    let (_, body, cookies) = t
        .raw(
            "POST",
            "/api/v1/auth/register",
            &[],
            Some(json!({ "username": "cookieman", "email": "cookieman@example.com", "password": "password123" })),
        )
        .await;
    assert!(body["access_token"].as_str().is_some());

    let live: Vec<&String> = cookies
        .iter()
        .filter(|c| !c.contains("Max-Age=0"))
        .collect();
    assert_eq!(live.len(), 1, "only one cookie stays set: {cookies:?}");
    let c = live[0];
    assert!(c.starts_with("refresh_token="));
    for attr in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/api/v1/auth"] {
        assert!(c.contains(attr), "{attr} missing from {c}");
    }
    // The old site-wide cookies are retired.
    for legacy in ["token=;", "lang=;", "timestamp=;"] {
        assert!(cookies.iter().any(|c| c.starts_with(legacy)), "{legacy}");
    }
    assert_eq!(session_count(&t, false).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_cookie_no_longer_authenticates_only_bearer_does(pool: PgPool) {
    let t = app(pool);
    let (_, access, _) = t.register("password123").await;
    assert_eq!(t.me(&access).await, StatusCode::OK);

    let (status, _) = t
        .call(
            "GET",
            "/api/v1/auth/me",
            &[("cookie", format!("token={access}"))],
            None,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the access token as a cookie"
    );
    // A refresh token is not an access token.
    let (_, _, refresh) = t.register("password123").await;
    assert_eq!(t.me(&refresh).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn refreshing_rotates_the_token_and_the_new_one_works(pool: PgPool) {
    let t = app(pool);
    let (_, _, first) = t.register("password123").await;

    let (status, body, cookies) = t.refresh(&first).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let second = refresh_cookie(&cookies).expect("a new refresh cookie");
    assert_ne!(first, second, "the token is replaced, not reused");
    assert_eq!(
        t.me(body["access_token"].as_str().unwrap()).await,
        StatusCode::OK
    );

    let (status, _, cookies) = t.refresh(&second).await;
    assert_eq!(status, StatusCode::OK, "the new token works");
    assert!(refresh_cookie(&cookies).is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_tabs_racing_one_refresh_do_not_sign_anyone_out(pool: PgPool) {
    let t = app(pool);
    let (_, _, first) = t.register("password123").await;

    // Tab A wins; tab B arrives with the same (now spent) cookie moments later.
    let (a, _, a_cookies) = t.refresh(&first).await;
    let (b, b_body, b_cookies) = t.refresh(&first).await;
    assert_eq!(a, StatusCode::OK);
    assert_eq!(b, StatusCode::OK, "inside the grace window");
    assert!(b_body["access_token"].as_str().is_some());
    assert!(
        refresh_cookie(&b_cookies).is_none() && b_body.get("refresh_token").is_none(),
        "tab B gets no second refresh token: it already shares tab A's cookie"
    );
    assert_eq!(session_count(&t, true).await, 0);

    // The cookie tab A received is the live one.
    let live = refresh_cookie(&a_cookies).unwrap();
    assert_eq!(t.refresh(&live).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_refreshes_never_revoke_the_session(pool: PgPool) {
    let t = app(pool);
    let (_, _, first) = t.register("password123").await;
    let (r1, r2, r3) = tokio::join!(t.refresh(&first), t.refresh(&first), t.refresh(&first));
    for (status, _, _) in [&r1, &r2, &r3] {
        assert_eq!(*status, StatusCode::OK);
    }
    let rotated = [&r1, &r2, &r3]
        .iter()
        .filter(|(_, _, c)| refresh_cookie(c).is_some())
        .count();
    assert_eq!(rotated, 1, "exactly one request rotates the token");
    assert_eq!(session_count(&t, true).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_spent_token_replayed_later_revokes_the_whole_session(pool: PgPool) {
    let t = app(pool);
    let (_, _, first) = t.register("password123").await;
    let (_, _, cookies) = t.refresh(&first).await;
    let second = refresh_cookie(&cookies).unwrap();

    // Past the grace window, the old token coming back means someone copied it.
    sqlx::query("UPDATE refresh_sessions SET rotated_at = NOW() - INTERVAL '1 minute'")
        .execute(&t.pool)
        .await
        .unwrap();
    let (status, _, cookies) = t.refresh(&first).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("refresh_token=;") && c.contains("Max-Age=0")),
        "the dead cookie is cleared"
    );
    // ...and the legitimate holder of the newest token is signed out too.
    assert_eq!(t.refresh(&second).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(session_count(&t, true).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_revokes_the_session_behind_the_cookie(pool: PgPool) {
    let t = app(pool);
    let (_, _, refresh) = t.register("password123").await;

    let (status, _, cookies) = t
        .raw(
            "POST",
            "/api/v1/auth/logout",
            &[("cookie", format!("refresh_token={refresh}"))],
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("refresh_token=;") && c.contains("Path=/api/v1/auth"))
    );
    assert_eq!(
        t.refresh(&refresh).await.0,
        StatusCode::UNAUTHORIZED,
        "the copy is dead too"
    );
    assert_eq!(session_count(&t, true).await, 1);

    // Logging out with nothing (or garbage) still succeeds and clears the cookie.
    let (status, _, _) = t.raw("POST", "/api/v1/auth/logout", &[], None).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn logging_out_one_device_leaves_the_others_signed_in(pool: PgPool) {
    let t = app(pool);
    let (email, _, laptop) = t.register("password123").await;
    let (_, body) = t.login(&email, "password123").await;
    let phone = body["refresh_token"].as_str().unwrap().to_string();

    t.raw(
        "POST",
        "/api/v1/auth/logout",
        &[("cookie", format!("refresh_token={laptop}"))],
        None,
    )
    .await;
    assert_eq!(t.refresh(&laptop).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(t.refresh(&phone).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cookie_endpoints_refuse_a_foreign_origin(pool: PgPool) {
    let t = app(pool);
    let (_, _, refresh) = t.register("password123").await;
    let cookie = ("cookie", format!("refresh_token={refresh}"));

    for path in ["/api/v1/auth/refresh", "/api/v1/auth/logout"] {
        let (status, _, _) = t
            .raw(
                "POST",
                path,
                &[cookie.clone(), ("origin", "https://evil.test".into())],
                None,
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
    // The refused calls changed nothing; the site's own origin is fine.
    let (status, _, _) = t
        .raw(
            "POST",
            "/api/v1/auth/refresh",
            &[cookie, ("origin", "https://app.test".into())],
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refresh_token_from_before_sessions_existed_is_rejected(pool: PgPool) {
    let t = app(pool);
    // Signed by the real secret, but with no session or token id: an old token.
    let claims = json!({
        "sub": Uuid::new_v4(), "username": "old", "token_type": "Refresh",
        "exp": 4_000_000_000u64, "iat": 1_700_000_000u64
    });
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS512),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(b"test-refresh-secret"),
    )
    .unwrap();
    assert_eq!(t.refresh(&token).await.0, StatusCode::UNAUTHORIZED);
}

/// Plant a one-time code the way the service stores one.
async fn plant_code(t: &App, email: &str, kind: &str, code: &str, seconds_from_now: i32) {
    let user_id = t.user_id(email).await;
    sqlx::query(
        "INSERT INTO auth_codes (code_hash, kind, user_id, expires_at)
         VALUES ($1, $2, $3, NOW() + make_interval(secs => $4::int))",
    )
    .bind(hash_reset_token(code))
    .bind(kind)
    .bind(user_id)
    .bind(seconds_from_now)
    .execute(&t.pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_google_code_signs_in_once(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register("password123").await;
    plant_code(&t, &email, "oauth", "code-1", 60).await;

    let (status, body, cookies) = t
        .raw(
            "POST",
            "/api/v1/auth/oauth/exchange",
            &[],
            Some(json!({ "code": "code-1" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["user"]["email"], email);
    let access = body["access_token"].as_str().unwrap();
    assert_eq!(t.me(access).await, StatusCode::OK);
    assert!(refresh_cookie(&cookies).is_some());
    // The sign-in from registering, plus the new one from the exchange.
    assert_eq!(session_count(&t, false).await, 2);

    // Single use.
    let (status, _) = t
        .post("/api/v1/auth/oauth/exchange", json!({ "code": "code-1" }))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_google_code_must_be_fresh_and_of_the_right_kind(pool: PgPool) {
    let t = app(pool);
    let (email, _, _) = t.register("password123").await;
    plant_code(&t, &email, "oauth", "stale", -1).await;
    plant_code(&t, &email, "ws", "ticket", 60).await;

    for code in ["stale", "ticket", "never-issued", ""] {
        let (status, _) = t
            .post("/api/v1/auth/oauth/exchange", json!({ "code": code }))
            .await;
        assert!(
            status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST,
            "{code:?} -> {status}"
        );
    }
    // A chat ticket cannot be traded for a session, and was not spent by trying.
    let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM auth_codes WHERE kind = 'ws'")
        .fetch_one(&t.pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_password_reset_revokes_every_session(pool: PgPool) {
    let t = app(pool);
    let (email, _, laptop) = t.register("password123").await;
    let (_, body) = t.login(&email, "password123").await;
    let phone = body["refresh_token"].as_str().unwrap().to_string();

    t.plant_token(&email, "reset-me", 30).await;
    let (status, _) = t
        .post(
            "/api/v1/auth/reset-password",
            json!({ "token": "reset-me", "new_password": "brand-new-pass" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(t.refresh(&laptop).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(t.refresh(&phone).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(session_count(&t, false).await, 0);
}
