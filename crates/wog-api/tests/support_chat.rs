//! Support chat over a real WebSocket: a server is started on an ephemeral
//! port for each test, backed by its own freshly migrated database from
//! `#[sqlx::test]` (so `DATABASE_URL` must point at a Postgres where the user
//! may create databases).

use std::{net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, Message, client::IntoClientRequest, http::HeaderValue},
};
use tower::ServiceExt;
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;
use wog_api::{
    api_routes,
    routers::{oauth_routes, support_ws_routes},
};
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
        admin::AdminServices, billing::BillingServices, marketplace::MarketplaceServices,
        oauth::OAuthServices, support::SupportServices, users::UserServices,
    },
    support::{HubConfig, LocalHub, SupportHub},
};
use wog_middleware::AppState;

const ORIGIN: &str = "https://chat.test";

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
        client_url: ORIGIN.into(),
        rest_api_url: ORIGIN.into(),
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

struct Harness {
    addr: SocketAddr,
    app: Router,
    state: AppState,
    hub: Arc<LocalHub>,
    pool: PgPool,
}

async fn harness(pool: PgPool) -> Harness {
    harness_with(pool, HubConfig::default()).await
}

async fn harness_with(pool: PgPool, hub_cfg: HubConfig) -> Harness {
    let cfg = test_config();
    let hub = Arc::new(LocalHub::new(hub_cfg));
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
            hub.clone(),
        ),
    };
    let (api, _) = OpenApiRouter::<AppState>::new()
        .merge(api_routes())
        .split_for_parts();
    let app = Router::new()
        .merge(oauth_routes())
        .merge(support_ws_routes())
        .merge(api)
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serve = app.clone();
    tokio::spawn(async move { axum::serve(listener, serve).await.unwrap() });
    Harness {
        addr,
        app,
        state,
        hub,
        pool,
    }
}

struct Account {
    id: Uuid,
    token: String,
}

impl Harness {
    async fn account(&self, role: &str, verified: bool) -> Account {
        let name = format!("u{}", &Uuid::new_v4().simple().to_string()[..12]);
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO users (username, email, email_verified, role) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(&name)
        .bind(format!("{name}@example.com"))
        .bind(verified)
        .bind(role)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        let user = self.state.user_services.get_user(id).await.unwrap();
        let token = self
            .state
            .app_config
            .generate_token_pair(&user)
            .unwrap()
            .access_token;
        Account { id, token }
    }

    async fn user(&self) -> Account {
        self.account("user", true).await
    }

    async fn admin(&self) -> Account {
        self.account("admin", true).await
    }

    fn request(&self, token: Option<&str>, origin: Option<&str>) -> tungstenite::http::Request<()> {
        let mut req = format!("ws://{}/api/v1/support/ws", self.addr)
            .into_client_request()
            .unwrap();
        if let Some(t) = token {
            req.headers_mut().insert(
                "Cookie",
                HeaderValue::from_str(&format!("token={t}")).unwrap(),
            );
        }
        if let Some(o) = origin {
            req.headers_mut()
                .insert("Origin", HeaderValue::from_str(o).unwrap());
        }
        req
    }

    /// Connect as `account` the way a browser would (cookie + allowed Origin)
    /// and consume the `ready` frame.
    async fn connect(&self, account: &Account) -> Ws {
        let (sock, _) = connect_async(self.request(Some(&account.token), Some(ORIGIN)))
            .await
            .expect("upgrade");
        let mut ws = Ws { sock };
        let ready = ws.next().await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["user_id"], account.id.to_string());
        ws
    }

    /// REST call as `account` (or anonymously).
    async fn rest(
        &self,
        method: &str,
        uri: &str,
        account: Option<&Account>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(a) = account {
            req = req.header("Authorization", format!("Bearer {}", a.token));
        }
        let res = self
            .app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn scalar_i64(&self, sql: &str) -> i64 {
        let (n,): (i64,) = sqlx::query_as(sql).fetch_one(&self.pool).await.unwrap();
        n
    }
}

struct Ws {
    sock: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Ws {
    async fn send(&mut self, value: Value) {
        self.send_raw(&value.to_string()).await;
    }

    async fn send_raw(&mut self, text: &str) {
        self.sock
            .send(Message::Text(text.to_string().into()))
            .await
            .unwrap();
    }

    /// The next JSON event; protocol pings are skipped.
    async fn next(&mut self) -> Value {
        loop {
            let msg = timeout(Duration::from_secs(5), self.sock.next())
                .await
                .expect("timed out waiting for an event")
                .expect("socket closed")
                .expect("socket error");
            if let Message::Text(t) = msg {
                return serde_json::from_str(t.as_str()).unwrap();
            }
        }
    }

    /// Skip events until one of this type arrives.
    async fn until(&mut self, ty: &str) -> Value {
        loop {
            let event = self.next().await;
            if event["type"] == ty {
                return event;
            }
        }
    }

    /// Everything that arrives in the next `ms`.
    async fn drain(&mut self, ms: u64) -> Vec<Value> {
        let mut out = vec![];
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.sock.next()).await {
                Err(_) | Ok(None) | Ok(Some(Err(_))) => return out,
                Ok(Some(Ok(Message::Text(t)))) => {
                    out.push(serde_json::from_str(t.as_str()).unwrap())
                }
                Ok(Some(Ok(_))) => {}
            }
        }
    }

    async fn say(&mut self, body: &str, id: &str) -> Value {
        self.send(json!({"type": "send_message", "body": body, "client_msg_id": id}))
            .await;
        self.until_ack_or_error(id).await
    }

    async fn say_to(&mut self, conversation: &str, body: &str, id: &str) -> Value {
        self.send(json!({"type": "send_message", "conversation_id": conversation, "body": body, "client_msg_id": id}))
            .await;
        self.until_ack_or_error(id).await
    }

    async fn until_ack_or_error(&mut self, id: &str) -> Value {
        loop {
            let e = self.next().await;
            if (e["type"] == "ack" || e["type"] == "error") && e["client_msg_id"] == id {
                return e;
            }
        }
    }

    async fn ping(&mut self) {
        self.send(json!({"type": "ping"})).await;
        self.until("pong").await;
    }

    async fn join(&mut self, conversation: &str) {
        self.send(json!({"type": "join_conversation", "conversation_id": conversation}))
            .await;
        // Events are handled in order, so once the pong is back the join is in.
        self.ping().await;
    }
}

fn conv_id(ack: &Value) -> String {
    ack["conversation_id"].as_str().unwrap().to_string()
}

async fn wait_for(mut cond: impl FnMut() -> bool) {
    for _ in 0..100 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("condition not met in time");
}

fn count(events: &[Value], ty: &str) -> usize {
    events.iter().filter(|e| e["type"] == ty).count()
}

// ───────────────────────── upgrade: auth and origin ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn upgrade_needs_auth_and_an_allowed_origin(pool: PgPool) {
    let h = harness(pool).await;
    let user = h.user().await;

    let status = |r: Result<_, tungstenite::Error>| match r {
        Err(tungstenite::Error::Http(res)) => res.status().as_u16(),
        Ok(_) => 101,
        Err(e) => panic!("unexpected: {e}"),
    };

    // No credentials.
    assert_eq!(
        status(connect_async(h.request(None, Some(ORIGIN))).await),
        401
    );
    // Garbage token.
    assert_eq!(
        status(connect_async(h.request(Some("not-a-jwt"), Some(ORIGIN))).await),
        401
    );
    // Valid cookie from a foreign site (cross-site WebSocket hijacking).
    assert_eq!(
        status(connect_async(h.request(Some(&user.token), Some("https://evil.test"))).await),
        403
    );
    assert_eq!(
        status(connect_async(h.request(Some(&user.token), Some("null"))).await),
        403
    );
    // The configured site, ALLOWED_ORIGINS entries (spaces tolerated), and a
    // non-browser client without Origin are accepted.
    for origin in [
        Some(ORIGIN),
        Some("https://extra.test"),
        Some("http://localhost:5173"),
        None,
    ] {
        assert_eq!(
            status(connect_async(h.request(Some(&user.token), origin)).await),
            101,
            "origin {origin:?}"
        );
    }
    // A bearer header works too (CLI).
    let mut req = h.request(None, None);
    req.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", user.token)).unwrap(),
    );
    assert_eq!(status(connect_async(req).await), 101);
}

#[sqlx::test(migrations = "../../migrations")]
async fn upgrade_refuses_unverified_users(pool: PgPool) {
    let h = harness(pool).await;
    let unverified = h.account("user", false).await;
    match connect_async(h.request(Some(&unverified.token), Some(ORIGIN))).await {
        Err(tungstenite::Error::Http(r)) => assert_eq!(r.status().as_u16(), 403),
        other => panic!("expected 403, got {:?}", other.map(|_| ())),
    }
}

// ───────────────────────── messaging ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn user_message_is_persisted_acked_and_delivered_to_admin(pool: PgPool) {
    let h = harness(pool).await;
    let (user, admin) = (h.user().await, h.admin().await);
    let mut a = h.connect(&admin).await;
    let mut u = h.connect(&user).await;

    // The admin sees the user come online.
    let presence = a.until("presence").await;
    assert_eq!(presence["user_id"], user.id.to_string());
    assert_eq!(presence["online"], true);

    let ack = u.say("  hello support  ", "m1").await;
    assert_eq!(ack["type"], "ack");
    assert_eq!(ack["duplicate"], false);
    let conversation_id = conv_id(&ack);

    // Persisted, and trimmed.
    let (body, role, cmid): (String, String, Option<String>) = sqlx::query_as(
        "SELECT body, sender_role, client_msg_id FROM support_messages WHERE id = $1::uuid",
    )
    .bind(ack["message_id"].as_str().unwrap())
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(
        (body.as_str(), role.as_str(), cmid.as_deref()),
        ("hello support", "user", Some("m1"))
    );

    // An admin who has not joined only gets the inbox row...
    let updated = a.until("conversation_updated").await;
    assert_eq!(updated["conversation"]["id"], conversation_id);
    assert_eq!(
        updated["conversation"]["last_message_preview"],
        "hello support"
    );
    assert_eq!(updated["conversation"]["admin_unread_count"], 1);
    assert_eq!(updated["conversation"]["online"], true);
    assert_eq!(updated["total_unread"], 1);
    assert_eq!(count(&a.drain(200).await, "message"), 0);

    // ...and the messages once they join.
    a.join(&conversation_id).await;
    u.say("second", "m2").await;
    let msg = a.until("message").await;
    assert_eq!(msg["message"]["body"], "second");
    assert_eq!(msg["message"]["sender_role"], "user");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_reply_reaches_only_that_user(pool: PgPool) {
    let h = harness(pool).await;
    let (alice, bob, admin) = (h.user().await, h.user().await, h.admin().await);
    let mut a = h.connect(&alice).await;
    let mut b = h.connect(&bob).await;
    let mut adm = h.connect(&admin).await;

    let conv_a = conv_id(&a.say("alice here", "a1").await);
    b.say("bob here", "b1").await;

    let ack = adm.say_to(&conv_a, "hi alice", "r1").await;
    assert_eq!(ack["type"], "ack");

    // Alice first sees her own message echoed to all her tabs, then the reply.
    let mut reply = a.until("message").await;
    if reply["message"]["sender_role"] != "admin" {
        reply = a.until("message").await;
    }
    assert_eq!(reply["message"]["body"], "hi alice");

    // Bob never sees anything about Alice's conversation.
    let bobs = b.drain(400).await;
    for e in &bobs {
        let text = e.to_string();
        assert!(
            !text.contains("alice here") && !text.contains("hi alice"),
            "leak: {text}"
        );
    }
    assert!(
        bobs.iter()
            .filter(|e| e["type"] == "message")
            .all(|e| e["message"]["sender_id"] == bob.id.to_string())
    );
    assert!(
        bobs.iter()
            .filter(|e| e["type"] == "conversation_updated")
            .all(|e| e["conversation"]["user_id"] == bob.id.to_string())
    );

    // The reply is unread for Alice until she reads it.
    let (_, view) = h
        .rest("GET", "/api/v1/support/conversation", Some(&alice))
        .await;
    assert_eq!(view["conversation"]["user_unread_count"], 1);
    a.send(json!({"type": "mark_read"})).await;
    assert_eq!(adm.until("read").await["by"], "user");
    let (_, view) = h
        .rest("GET", "/api/v1/support/conversation", Some(&alice))
        .await;
    assert_eq!(view["conversation"]["user_unread_count"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn typing_goes_to_the_other_side_only(pool: PgPool) {
    let h = harness(pool).await;
    let (alice, bob, admin) = (h.user().await, h.user().await, h.admin().await);
    let mut a = h.connect(&alice).await;
    let mut b = h.connect(&bob).await;
    let mut adm = h.connect(&admin).await;
    let conv = conv_id(&a.say("x", "a1").await);
    b.say("y", "b1").await;
    adm.join(&conv).await;

    a.send(json!({"type": "typing", "is_typing": true})).await;
    let t = adm.until("typing").await;
    assert_eq!(t["role"], "user");
    assert_eq!(t["is_typing"], true);

    adm.send(json!({"type": "typing", "conversation_id": conv, "is_typing": true}))
        .await;
    assert_eq!(a.until("typing").await["role"], "admin");
    assert_eq!(count(&b.drain(300).await, "typing"), 0);

    // A user cannot type into someone else's conversation.
    b.send(json!({"type": "typing", "conversation_id": conv, "is_typing": true}))
        .await;
    assert_eq!(b.until("error").await["code"], "not_found");
    assert_eq!(count(&adm.drain(300).await, "typing"), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_msg_id_makes_sends_idempotent(pool: PgPool) {
    let h = harness(pool).await;
    let (user, admin) = (h.user().await, h.admin().await);
    let mut u = h.connect(&user).await;
    let first = u.say("once", "same-id").await;
    let mut adm = h.connect(&admin).await;
    adm.join(&conv_id(&first)).await;

    // A reconnecting client resends what it never saw acked.
    let mut u2 = h.connect(&user).await;
    let again = u2.say("once", "same-id").await;
    assert_eq!(again["duplicate"], true);
    assert_eq!(again["message_id"], first["message_id"]);

    assert_eq!(
        h.scalar_i64("SELECT COUNT(*) FROM support_messages").await,
        1
    );
    // Nobody was told about it a second time, and it counts as one unread.
    assert_eq!(count(&adm.drain(300).await, "message"), 0);
    let (_, inbox) = h
        .rest("GET", "/api/v1/support/conversations", Some(&admin))
        .await;
    assert_eq!(inbox["conversations"][0]["admin_unread_count"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_tabs_and_two_admins_all_stay_in_sync(pool: PgPool) {
    let h = harness(pool).await;
    let (user, admin) = (h.user().await, h.admin().await);
    let mut tab1 = h.connect(&user).await;
    let mut tab2 = h.connect(&user).await;
    let mut adm1 = h.connect(&admin).await;
    let mut adm2 = h.connect(&admin).await;
    let conv = conv_id(&tab1.say("from tab one", "t1").await);

    // The other tab gets the message too.
    assert_eq!(
        tab2.until("message").await["message"]["body"],
        "from tab one"
    );
    // Both admin tabs get the inbox row.
    assert_eq!(
        adm1.until("conversation_updated").await["conversation"]["id"],
        conv
    );
    assert_eq!(
        adm2.until("conversation_updated").await["conversation"]["id"],
        conv
    );

    // Presence stays "online" until the LAST tab closes.
    drop(tab1);
    wait_for(|| h.hub.connection_count() == 3).await;
    assert!(
        adm1.drain(200)
            .await
            .iter()
            .all(|e| e["type"] != "presence" || e["online"] == true)
    );
    drop(tab2);
    let gone = loop {
        let e = adm1.until("presence").await;
        if e["online"] == false {
            break e;
        }
    };
    assert_eq!(gone["user_id"], user.id.to_string());
}

// ───────────────────────── permissions ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn non_admins_cannot_use_admin_events_or_endpoints(pool: PgPool) {
    let h = harness(pool).await;
    let (alice, bob, admin) = (h.user().await, h.user().await, h.admin().await);
    let mut a = h.connect(&alice).await;
    let conv = conv_id(&a.say("help", "a1").await);

    for event in [
        json!({"type": "join_conversation", "conversation_id": conv}),
        json!({"type": "close_conversation", "conversation_id": conv}),
        json!({"type": "reopen_conversation", "conversation_id": conv}),
        json!({"type": "assign_to_me", "conversation_id": conv}),
        json!({"type": "leave_conversation", "conversation_id": conv}),
    ] {
        a.send(event.clone()).await;
        assert_eq!(a.until("error").await["code"], "forbidden", "{event}");
    }
    let (status,): (String,) = sqlx::query_as("SELECT status FROM support_conversations")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(status, "open");

    // REST: admin-only routes.
    for (method, uri) in [
        ("GET", "/api/v1/support/conversations".to_string()),
        ("GET", format!("/api/v1/support/conversations/{conv}")),
        (
            "POST",
            format!("/api/v1/support/conversations/{conv}/close"),
        ),
        (
            "POST",
            format!("/api/v1/support/conversations/{conv}/reopen"),
        ),
    ] {
        let (s, _) = h.rest(method, &uri, Some(&bob)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{method} {uri}");
        let (s, _) = h.rest(method, &uri, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    // Someone else's conversation does not exist for Bob (404, not 403)...
    let msgs = format!("/api/v1/support/conversations/{conv}/messages");
    assert_eq!(
        h.rest("GET", &msgs, Some(&bob)).await.0,
        StatusCode::NOT_FOUND
    );
    // ...and is readable by its owner and by an admin.
    assert_eq!(h.rest("GET", &msgs, Some(&alice)).await.0, StatusCode::OK);
    assert_eq!(h.rest("GET", &msgs, Some(&admin)).await.0, StatusCode::OK);

    // Bob cannot write into it over the socket either, nor mark it read.
    let mut b = h.connect(&bob).await;
    b.send(json!({"type": "send_message", "conversation_id": conv, "body": "hijack", "client_msg_id": "x"}))
        .await;
    assert_eq!(b.until("error").await["code"], "not_found");
    b.send(json!({"type": "mark_read", "conversation_id": conv}))
        .await;
    assert_eq!(b.until("error").await["code"], "not_found");
    assert_eq!(
        h.scalar_i64("SELECT COUNT(*) FROM support_messages WHERE body = 'hijack'")
            .await,
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_demoted_admin_loses_admin_powers_immediately(pool: PgPool) {
    let h = harness(pool).await;
    let (user, admin) = (h.user().await, h.admin().await);
    let mut u = h.connect(&user).await;
    let conv = conv_id(&u.say("hi", "u1").await);
    let mut adm = h.connect(&admin).await;
    adm.ping().await;

    sqlx::query("UPDATE users SET role = 'user' WHERE id = $1")
        .bind(admin.id)
        .execute(&h.pool)
        .await
        .unwrap();

    // The token says nothing about roles; the database decides.
    adm.send(json!({"type": "send_message", "conversation_id": conv, "body": "still here?", "client_msg_id": "z"}))
        .await;
    assert_eq!(adm.until("error").await["code"], "forbidden");
    let (s, _) = h
        .rest("GET", "/api/v1/support/conversations", Some(&admin))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(
        h.scalar_i64("SELECT COUNT(*) FROM support_messages WHERE sender_role = 'admin'")
            .await,
        0
    );
}

// ───────────────────────── input limits ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn bad_input_is_rejected_without_dropping_the_connection(pool: PgPool) {
    let h = harness(pool).await;
    let user = h.user().await;
    let mut u = h.connect(&user).await;

    // Over-long body.
    let e = u.say(&"x".repeat(2001), "long").await;
    assert_eq!(e["type"], "error");
    assert_eq!(e["code"], "message_too_long");
    // Exactly the limit is fine (characters, not bytes).
    assert_eq!(u.say(&"é".repeat(2000), "max").await["type"], "ack");
    // Empty / whitespace-only.
    assert_eq!(u.say("   \n\t ", "blank").await["code"], "empty_message");
    // Bad client_msg_id.
    assert_eq!(u.say("hi", "").await["code"], "invalid_client_msg_id");

    // Malformed JSON, unknown type, missing fields, wrong types.
    for raw in [
        "{not json",
        "[]",
        "\"str\"",
        r#"{"type":"nope"}"#,
        r#"{"type":"send_message"}"#,
        r#"{"type":"typing","is_typing":"yes"}"#,
        "",
    ] {
        u.send_raw(raw).await;
        assert_eq!(u.until("error").await["code"], "bad_request", "{raw}");
    }
    u.sock
        .send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert_eq!(u.until("error").await["code"], "bad_request");

    // Still alive, and nothing bad was stored.
    u.ping().await;
    assert_eq!(
        h.scalar_i64("SELECT COUNT(*) FROM support_messages").await,
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_oversized_frame_closes_the_socket_cleanly(pool: PgPool) {
    let h = harness(pool).await;
    let user = h.user().await;
    let mut u = h.connect(&user).await;
    let _ = u
        .sock
        .send(Message::Text("x".repeat(64 * 1024).into()))
        .await;
    // The protocol layer refuses it; the connection ends and the hub is
    // cleaned up (no panic, no leaked sender).
    wait_for(|| h.hub.connection_count() == 0).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn sending_too_fast_is_rate_limited(pool: PgPool) {
    let h = harness(pool).await;
    let user = h.user().await;
    let mut u = h.connect(&user).await;
    let (mut ok, mut limited) = (0, 0);
    for i in 0..14 {
        let r = u.say("spam", &format!("s{i}")).await;
        match r["code"].as_str() {
            Some("rate_limited") => limited += 1,
            _ => ok += 1,
        }
    }
    assert_eq!((ok, limited), (10, 4));
    // The connection survives, and rejected messages were not stored.
    u.ping().await;
    assert_eq!(
        h.scalar_i64("SELECT COUNT(*) FROM support_messages").await,
        10
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn connections_per_user_are_capped(pool: PgPool) {
    let h = harness_with(
        pool,
        HubConfig {
            max_user_connections: 2,
            ..HubConfig::default()
        },
    )
    .await;
    let user = h.user().await;
    let _a = h.connect(&user).await;
    let _b = h.connect(&user).await;
    let (sock, _) = connect_async(h.request(Some(&user.token), Some(ORIGIN)))
        .await
        .unwrap();
    let mut third = Ws { sock };
    assert_eq!(third.next().await["code"], "too_many_connections");
    assert_eq!(h.hub.connection_count(), 2);
}

// ───────────────────────── reconnect, history, lifecycle ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn reconnect_catches_up_without_gaps_or_duplicates(pool: PgPool) {
    let h = harness(pool).await;
    let (user, admin) = (h.user().await, h.admin().await);
    let mut u = h.connect(&user).await;
    let mut adm = h.connect(&admin).await;
    let ack = u.say("m0", "c0").await;
    let conv = conv_id(&ack);
    let first = ack["message_id"].as_str().unwrap().to_string();

    // The user's connection drops; replies pile up meanwhile.
    drop(u);
    for i in 1..=5 {
        let r = adm
            .say_to(&conv, &format!("reply {i}"), &format!("r{i}"))
            .await;
        assert_eq!(r["type"], "ack");
    }

    // On reconnect the widget asks for everything after the last id it saw.
    let _u = h.connect(&user).await;
    let uri = format!("/api/v1/support/conversations/{conv}/messages?after={first}&limit=3");
    let (s, page1) = h.rest("GET", &uri, Some(&user)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(page1["has_more"], true);
    let last = page1["messages"][2]["id"].as_str().unwrap().to_string();
    let uri = format!("/api/v1/support/conversations/{conv}/messages?after={last}&limit=3");
    let (_, page2) = h.rest("GET", &uri, Some(&user)).await;
    assert_eq!(page2["has_more"], false);

    let bodies: Vec<&str> = page1["messages"]
        .as_array()
        .unwrap()
        .iter()
        .chain(page2["messages"].as_array().unwrap())
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(
        bodies,
        ["reply 1", "reply 2", "reply 3", "reply 4", "reply 5"]
    );

    // Scrolling back works from the newest page, with no overlap.
    let (_, newest) = h
        .rest(
            "GET",
            &format!("/api/v1/support/conversations/{conv}/messages?limit=4"),
            Some(&user),
        )
        .await;
    assert_eq!(newest["has_more"], true);
    let oldest_id = newest["messages"][0]["id"].as_str().unwrap();
    let (_, older) = h
        .rest(
            "GET",
            &format!("/api/v1/support/conversations/{conv}/messages?limit=4&before={oldest_id}"),
            Some(&user),
        )
        .await;
    assert_eq!(older["has_more"], false);
    let all: Vec<&str> = older["messages"]
        .as_array()
        .unwrap()
        .iter()
        .chain(newest["messages"].as_array().unwrap())
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(
        all,
        ["m0", "reply 1", "reply 2", "reply 3", "reply 4", "reply 5"]
    );

    // The user's own view agrees, and an unknown cursor is refused.
    let (_, mine) = h
        .rest("GET", "/api/v1/support/conversation", Some(&user))
        .await;
    assert_eq!(mine["messages"].as_array().unwrap().len(), 6);
    let (s, _) = h
        .rest(
            "GET",
            &format!(
                "/api/v1/support/conversations/{conv}/messages?after={}",
                Uuid::new_v4()
            ),
            Some(&user),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn disconnects_leave_nothing_behind(pool: PgPool) {
    let h = harness(pool).await;
    let (user, admin) = (h.user().await, h.admin().await);
    let mut sockets = vec![];
    for _ in 0..3 {
        sockets.push(h.connect(&user).await);
    }
    sockets.push(h.connect(&admin).await);
    assert_eq!(h.hub.connection_count(), 4);
    assert!(h.hub.online_users(&[user.id]).contains(&user.id));

    // A clean close, then an abrupt drop, then everything else.
    let mut first = sockets.remove(0);
    first.sock.close(None).await.unwrap();
    drop(sockets.remove(0));
    wait_for(|| h.hub.connection_count() == 2).await;
    sockets.clear();
    wait_for(|| h.hub.connection_count() == 0).await;
    assert!(h.hub.online_users(&[user.id, admin.id]).is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn closing_and_reopening_keeps_one_thread_per_user(pool: PgPool) {
    let h = harness(pool).await;
    let (user, admin) = (h.user().await, h.admin().await);
    let mut u = h.connect(&user).await;
    let mut adm = h.connect(&admin).await;
    let conv = conv_id(&u.say("problem", "p1").await);

    // Admin closes over the socket; the user is told.
    adm.send(json!({"type": "close_conversation", "conversation_id": conv}))
        .await;
    loop {
        let e = u.until("conversation_updated").await;
        if e["conversation"]["status"] == "closed" {
            break;
        }
    }
    // A reply to a closed conversation is refused until it is reopened.
    assert_eq!(
        adm.say_to(&conv, "too late", "r1").await["code"],
        "conversation_closed"
    );

    // The user can still read it, and writing again reopens the SAME thread.
    let (_, view) = h
        .rest("GET", "/api/v1/support/conversation", Some(&user))
        .await;
    assert_eq!(view["conversation"]["status"], "closed");
    assert_eq!(u.say("one more thing", "p2").await["conversation_id"], conv);
    let (_, view) = h
        .rest("GET", "/api/v1/support/conversation", Some(&user))
        .await;
    assert_eq!(view["conversation"]["status"], "open");
    assert_eq!(view["messages"].as_array().unwrap().len(), 2);
    assert_eq!(
        h.scalar_i64("SELECT COUNT(*) FROM support_conversations")
            .await,
        1
    );

    // REST close/reopen are idempotent.
    let uri = format!("/api/v1/support/conversations/{conv}");
    let post = |action: &str| format!("{uri}/{action}");
    assert_eq!(
        h.rest("POST", &post("close"), Some(&admin)).await.1["status"],
        "closed"
    );
    assert_eq!(
        h.rest("POST", &post("close"), Some(&admin)).await.1["status"],
        "closed"
    );
    assert_eq!(
        h.rest("POST", &post("reopen"), Some(&admin)).await.1["status"],
        "open"
    );
    let missing = format!("/api/v1/support/conversations/{}/close", Uuid::new_v4());
    assert_eq!(
        h.rest("POST", &missing, Some(&admin)).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_one_open_conversation_rule_is_enforced_by_the_database(pool: PgPool) {
    let h = harness(pool).await;
    let user = h.user().await;
    sqlx::query("INSERT INTO support_conversations (user_id) VALUES ($1)")
        .bind(user.id)
        .execute(&h.pool)
        .await
        .unwrap();
    let dup = sqlx::query("INSERT INTO support_conversations (user_id) VALUES ($1)")
        .bind(user.id)
        .execute(&h.pool)
        .await;
    assert!(dup.is_err());
    // Closed ones are not limited.
    sqlx::query("INSERT INTO support_conversations (user_id, status) VALUES ($1, 'closed')")
        .bind(user.id)
        .execute(&h.pool)
        .await
        .unwrap();
}

// ───────────────────────── inbox ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn inbox_lists_filters_searches_and_counts_unread(pool: PgPool) {
    let h = harness(pool).await;
    let (alice, bob, admin) = (h.user().await, h.user().await, h.admin().await);
    let mut a = h.connect(&alice).await;
    let mut b = h.connect(&bob).await;
    a.say("a1", "a1").await;
    a.say("a2", "a2").await;
    b.say("b1", "b1").await;

    let (s, inbox) = h
        .rest("GET", "/api/v1/support/conversations", Some(&admin))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(inbox["total"], 2);
    assert_eq!(inbox["total_unread"], 3);
    // Newest activity first (Bob wrote last), with user info and presence.
    let rows = inbox["conversations"].as_array().unwrap();
    assert_eq!(rows[0]["user_id"], bob.id.to_string());
    assert_eq!(rows[0]["online"], true);
    assert_eq!(rows[1]["admin_unread_count"], 2);
    assert!(rows[0]["email"].as_str().unwrap().ends_with("@example.com"));

    // Search by a fragment of the username.
    let name: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(alice.id)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    let (_, found) = h
        .rest(
            "GET",
            &format!("/api/v1/support/conversations?q={}", &name[2..8]),
            Some(&admin),
        )
        .await;
    assert_eq!(found["total"], 1);
    assert_eq!(found["conversations"][0]["user_id"], alice.id.to_string());
    // LIKE wildcards in the query are literal.
    let (_, none) = h
        .rest("GET", "/api/v1/support/conversations?q=%25", Some(&admin))
        .await;
    assert_eq!(none["total"], 0);

    // Status filter and validation.
    let bob_conv = rows[0]["id"].as_str().unwrap().to_string();
    h.rest(
        "POST",
        &format!("/api/v1/support/conversations/{bob_conv}/close"),
        Some(&admin),
    )
    .await;
    let (_, open) = h
        .rest(
            "GET",
            "/api/v1/support/conversations?status=open",
            Some(&admin),
        )
        .await;
    assert_eq!(open["total"], 1);
    assert_eq!(open["total_unread"], 2);
    let (_, closed) = h
        .rest(
            "GET",
            "/api/v1/support/conversations?status=closed",
            Some(&admin),
        )
        .await;
    assert_eq!(closed["total"], 1);
    let (s, _) = h
        .rest(
            "GET",
            "/api/v1/support/conversations?status=weird",
            Some(&admin),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // An admin reading a conversation clears its counter and tells the user.
    let mut adm = h.connect(&admin).await;
    let alice_conv = open["conversations"][0]["id"].as_str().unwrap().to_string();
    adm.send(json!({"type": "mark_read", "conversation_id": alice_conv}))
        .await;
    let upd = adm.until("conversation_updated").await;
    assert_eq!(upd["conversation"]["admin_unread_count"], 0);
    assert_eq!(upd["total_unread"], 0);
    assert_eq!(a.until("read").await["by"], "admin");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_user_with_no_conversation_gets_an_empty_view(pool: PgPool) {
    let h = harness(pool).await;
    let user = h.user().await;
    let (s, view) = h
        .rest("GET", "/api/v1/support/conversation", Some(&user))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(view["conversation"].is_null());
    assert_eq!(view["messages"], json!([]));
    // Typing / reading with no conversation is a quiet no-op, not an error.
    let mut u = h.connect(&user).await;
    u.send(json!({"type": "typing", "is_typing": true})).await;
    u.send(json!({"type": "mark_read"})).await;
    u.ping().await;
    assert_eq!(count(&u.drain(100).await, "error"), 0);
}
