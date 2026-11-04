//! Integration tests for plans, orders, SePay settlement and gateway limits.
//!
//! Each test gets its own freshly migrated database from `#[sqlx::test]`, so
//! they need `DATABASE_URL` to point at a Postgres the user can create
//! databases in.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use payment_client::{BankDetails, PaymentProvider, Transfer};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wog_infras::{
    billing::{BillingConfig, generate_payment_code},
    crawler::Crawler,
    errors::DatabaseError,
    models::{ApiKey, SettleOutcome},
    repos::{
        DynBillingRepository, DynMarketplaceRepository, billing::PgBillingRepo,
        marketplace::PgMarketplaceRepo,
    },
    services::{billing::BillingServices, marketplace::MarketplaceServices},
};

const WEBHOOK_KEY: &str = "test-webhook-key";

struct FakeProvider {
    bank: BankDetails,
    transfers: Vec<Transfer>,
}

#[async_trait]
impl PaymentProvider for FakeProvider {
    fn bank_details(&self) -> &BankDetails {
        &self.bank
    }
    fn qr_url(&self, amount_vnd: i64, transfer_content: &str) -> String {
        format!("https://qr.test/img?amount={amount_vnd}&des={transfer_content}")
    }
    fn can_list_transfers(&self) -> bool {
        true
    }
    async fn recent_transfers(
        &self,
        _since: DateTime<Utc>,
    ) -> payment_client::Result<Vec<Transfer>> {
        Ok(self.transfers.clone())
    }
}

fn billing_with(pool: &PgPool, transfers: Vec<Transfer>) -> BillingServices {
    let repo: DynBillingRepository = Arc::new(PgBillingRepo::new(pool.clone()));
    let provider: Arc<dyn PaymentProvider> = Arc::new(FakeProvider {
        bank: BankDetails {
            bank_name: "TestBank".into(),
            account_number: "0123456789".into(),
            account_holder: "WOG TEST".into(),
        },
        transfers,
    });
    let config = BillingConfig {
        webhook_api_key: WEBHOOK_KEY.into(),
        code_prefix: "WOG".into(),
        order_ttl: Duration::minutes(30),
    };
    BillingServices::new(repo, Some(provider), Some(config))
}

fn billing(pool: &PgPool) -> BillingServices {
    billing_with(pool, vec![])
}

fn gateway(pool: &PgPool) -> MarketplaceServices {
    let repo: DynMarketplaceRepository = Arc::new(PgMarketplaceRepo::new(pool.clone()));
    let crawler = Crawler::new("http://127.0.0.1:1", "secret").unwrap();
    MarketplaceServices::new(repo, crawler)
}

async fn make_user(pool: &PgPool, verified: bool) -> Uuid {
    let name = format!("u{}", &Uuid::new_v4().simple().to_string()[..12]);
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO users (username, email, email_verified) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&name)
    .bind(format!("{name}@example.com"))
    .bind(verified)
    .fetch_one(pool)
    .await
    .unwrap();
    id
}

fn webhook_body(id: i64, amount: i64, content: &str, kind: &str) -> Vec<u8> {
    json!({
        "id": id,
        "gateway": "TestBank",
        "transactionDate": "2026-10-08 10:00:00",
        "accountNumber": "0123456789",
        "code": null,
        "content": content,
        "transferType": kind,
        "transferAmount": amount,
        "accumulated": 0,
        "subAccount": null,
        "referenceCode": "FT0001",
        "description": content
    })
    .to_string()
    .into_bytes()
}

fn auth() -> String {
    format!("Apikey {WEBHOOK_KEY}")
}

async fn deliver(
    svc: &BillingServices,
    id: i64,
    amount: i64,
    content: &str,
) -> Result<SettleOutcome, DatabaseError> {
    svc.handle_webhook(Some(&auth()), &webhook_body(id, amount, content, "in"))
        .await
}

async fn status_of(pool: &PgPool, order_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM payment_orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn active_plan_slug(pool: &PgPool, user_id: Uuid) -> Option<String> {
    sqlx::query_scalar(
        "SELECT p.slug FROM subscriptions s JOIN plans p ON p.id = s.plan_id
         WHERE s.user_id = $1 AND s.status = 'active'",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// Buy and pay for `slug` in full.
async fn buy(pool: &PgPool, svc: &BillingServices, user: Uuid, slug: &str, tx: i64) {
    let order = svc.create_order(user, true, slug).await.unwrap().order;
    let outcome = deliver(svc, tx, i64::from(order.amount_vnd), &order.payment_code)
        .await
        .unwrap();
    assert!(
        matches!(outcome, SettleOutcome::Activated { .. }),
        "{outcome:?}"
    );
    assert_eq!(active_plan_slug(pool, user).await.as_deref(), Some(slug));
}

// ───────────────────────── seed & codes ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn seeds_three_plans_in_order(pool: PgPool) {
    let plans = billing(&pool).plans().await.unwrap();
    let slugs: Vec<_> = plans.iter().map(|p| p.slug.as_str()).collect();
    assert_eq!(slugs, ["basic", "pro", "ultra"]);
    assert!(plans.windows(2).all(|w| w[0].price_vnd < w[1].price_vnd));
}

#[test]
fn payment_codes_are_well_formed_and_distinct() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..2000 {
        let code = generate_payment_code("WOG");
        assert_eq!(code.len(), 11);
        assert!(code.starts_with("WOG"));
        assert!(
            code[3..]
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        );
        assert!(seen.insert(code));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn payment_code_is_unique_in_the_database(pool: PgPool) {
    let user = make_user(&pool, true).await;
    let svc = billing(&pool);
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    let other = make_user(&pool, true).await;
    let dup = sqlx::query(
        "INSERT INTO payment_orders (user_id, plan_id, amount_vnd, payment_code, expires_at)
         VALUES ($1, $2, 1, $3, NOW() + interval '1 hour')",
    )
    .bind(other)
    .bind(order.plan_id)
    .bind(&order.payment_code)
    .execute(&pool)
    .await;
    assert!(dup.is_err());
}

// ───────────────────────── webhook ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn webhook_rejects_missing_and_wrong_credentials(pool: PgPool) {
    let svc = billing(&pool);
    let body = webhook_body(1, 99000, "WOGAAAAAAAA", "in");
    for header in [
        None,
        Some("Apikey nope"),
        Some("Bearer test-webhook-key"),
        Some(""),
    ] {
        let err = svc.handle_webhook(header, &body).await.unwrap_err();
        assert!(matches!(err, DatabaseError::Unauthorized(_)), "{header:?}");
    }
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM payment_events").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn webhook_rejects_unreadable_body(pool: PgPool) {
    let err = billing(&pool)
        .handle_webhook(Some(&auth()), b"not json")
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::ValidationError(_)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn valid_payment_activates_the_plan(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "pro").await.unwrap().order;
    // Banks upper-case, add noise and sometimes lowercase the content.
    let content = format!("MBVCB.123.{}.CT tu A", order.payment_code.to_lowercase());
    let outcome = deliver(&svc, 1001, 299_000, &content).await.unwrap();
    match outcome {
        SettleOutcome::Activated {
            plan_slug,
            late,
            overpaid_vnd,
            ..
        } => {
            assert_eq!(plan_slug, "pro");
            assert!(!late);
            assert_eq!(overpaid_vnd, 0);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(status_of(&pool, order.id).await, "paid");
    assert_eq!(active_plan_slug(&pool, user).await.as_deref(), Some("pro"));
    let summary = svc.subscription(user).await.unwrap();
    assert_eq!(summary.usage.quota, Some(100_000));
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_delivery_does_not_activate_twice(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    deliver(&svc, 2001, 99_000, &order.payment_code)
        .await
        .unwrap();
    let end_before: DateTime<Utc> =
        sqlx::query_scalar("SELECT current_period_end FROM subscriptions WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    for _ in 0..3 {
        let again = deliver(&svc, 2001, 99_000, &order.payment_code)
            .await
            .unwrap();
        assert!(matches!(again, SettleOutcome::Duplicate), "{again:?}");
    }
    let end_after: DateTime<Utc> =
        sqlx::query_scalar("SELECT current_period_end FROM subscriptions WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(end_before, end_after);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM subscriptions").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn parallel_duplicate_deliveries_activate_once(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;

    let mut tasks = Vec::new();
    for _ in 0..8 {
        let svc = svc.clone();
        let code = order.payment_code.clone();
        tasks.push(tokio::spawn(async move {
            deliver(&svc, 3001, 99_000, &code).await
        }));
    }
    let mut activated = 0;
    for task in tasks {
        match task.await.unwrap().unwrap() {
            SettleOutcome::Activated { .. } => activated += 1,
            SettleOutcome::Duplicate => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(activated, 1);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM subscriptions").await, 1);
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM payment_orders WHERE status = 'paid'"
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_different_transfers_for_one_order_pay_it_once(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    let (a, b) = tokio::join!(
        deliver(&svc, 4001, 99_000, &order.payment_code),
        deliver(&svc, 4002, 99_000, &order.payment_code),
    );
    let outcomes = [a.unwrap(), b.unwrap()];
    let activated = outcomes
        .iter()
        .filter(|o| matches!(o, SettleOutcome::Activated { .. }))
        .count();
    let flagged = outcomes
        .iter()
        .filter(|o| matches!(o, SettleOutcome::AlreadyPaid))
        .count();
    assert_eq!((activated, flagged), (1, 1), "{outcomes:?}");
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM payment_events WHERE needs_review"
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn underpayment_is_recorded_but_not_activated(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    let outcome = deliver(&svc, 5001, 98_999, &order.payment_code)
        .await
        .unwrap();
    assert!(matches!(outcome, SettleOutcome::Underpaid), "{outcome:?}");
    assert_eq!(status_of(&pool, order.id).await, "pending");
    assert_eq!(active_plan_slug(&pool, user).await, None);
    let (outcome, review): (String, bool) = sqlx::query_as(
        "SELECT outcome, needs_review FROM payment_events WHERE sepay_transaction_id = 5001",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(outcome, "underpaid");
    assert!(review);

    // Topping up with a second transfer of the full price still works.
    let ok = deliver(&svc, 5002, 99_000, &order.payment_code)
        .await
        .unwrap();
    assert!(matches!(ok, SettleOutcome::Activated { .. }), "{ok:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn overpayment_activates_and_records_the_excess(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    let outcome = deliver(&svc, 6001, 150_000, &order.payment_code)
        .await
        .unwrap();
    match outcome {
        SettleOutcome::Activated { overpaid_vnd, .. } => assert_eq!(overpaid_vnd, 51_000),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        active_plan_slug(&pool, user).await.as_deref(),
        Some("basic")
    );
    let paid: i64 = sqlx::query_scalar("SELECT paid_amount_vnd FROM payment_orders WHERE id = $1")
        .bind(order.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(paid, 150_000);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_code_is_acknowledged_and_stored(pool: PgPool) {
    let svc = billing(&pool);
    let outcome = deliver(&svc, 7001, 99_000, "WOGZZZZZZZZ").await.unwrap();
    assert!(matches!(outcome, SettleOutcome::Unmatched), "{outcome:?}");
    let outcome = deliver(&svc, 7002, 99_000, "lunch with friends")
        .await
        .unwrap();
    assert!(matches!(outcome, SettleOutcome::Unmatched), "{outcome:?}");
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM payment_events").await, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn outgoing_transfers_are_ignored(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    let outcome = svc
        .handle_webhook(
            Some(&auth()),
            &webhook_body(8001, 99_000, &order.payment_code, "out"),
        )
        .await
        .unwrap();
    assert!(matches!(outcome, SettleOutcome::Ignored), "{outcome:?}");
    assert_eq!(status_of(&pool, order.id).await, "pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn late_payment_on_an_expired_order_still_activates(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    sqlx::query("UPDATE payment_orders SET expires_at = NOW() - interval '1 hour' WHERE id = $1")
        .bind(order.id)
        .execute(&pool)
        .await
        .unwrap();
    svc.sweep().await.unwrap();
    assert_eq!(status_of(&pool, order.id).await, "expired");

    match deliver(&svc, 9001, 99_000, &order.payment_code)
        .await
        .unwrap()
    {
        SettleOutcome::Activated { late, .. } => assert!(late),
        other => panic!("{other:?}"),
    }
    assert_eq!(status_of(&pool, order.id).await, "paid");
}

#[sqlx::test(migrations = "../../migrations")]
async fn payment_for_a_canceled_order_is_flagged_not_applied(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    svc.cancel_order(user, order.id).await.unwrap();
    let outcome = deliver(&svc, 9101, 99_000, &order.payment_code)
        .await
        .unwrap();
    assert!(
        matches!(outcome, SettleOutcome::CanceledOrder),
        "{outcome:?}"
    );
    assert_eq!(active_plan_slug(&pool, user).await, None);
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM payment_events WHERE needs_review"
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_delivery_is_stored(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    deliver(&svc, 9201, 99_000, &order.payment_code)
        .await
        .unwrap();
    deliver(&svc, 9201, 99_000, &order.payment_code)
        .await
        .unwrap();
    deliver(&svc, 9202, 5, "nothing").await.unwrap();
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM payment_events").await, 3);
    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM payment_events WHERE sepay_transaction_id = 9202")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(payload["transferAmount"], 5);
}

// ───────────────────────── orders ─────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn creating_an_order_again_reuses_the_pending_one(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let a = svc.create_order(user, true, "pro").await.unwrap();
    let b = svc.create_order(user, true, "pro").await.unwrap();
    assert_eq!(a.order.id, b.order.id);
    assert_eq!(a.transfer_content, b.transfer_content);
    assert!(a.qr_url.contains("amount=299000"));
    assert!(a.transfer_content.starts_with("WOG"));
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM payment_orders").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn parallel_order_creation_yields_one_pending_order(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let svc = svc.clone();
        tasks.push(tokio::spawn(async move {
            svc.create_order(user, true, "basic")
                .await
                .unwrap()
                .order
                .id
        }));
    }
    let mut ids = std::collections::HashSet::new();
    for task in tasks {
        ids.insert(task.await.unwrap());
    }
    assert_eq!(ids.len(), 1);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM payment_orders").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn orders_need_a_verified_email_and_a_real_plan(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, false).await;
    assert!(matches!(
        svc.create_order(user, false, "basic").await,
        Err(DatabaseError::Forbidden(_))
    ));
    assert!(matches!(
        svc.create_order(user, true, "platinum").await,
        Err(DatabaseError::ValueNotFound(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_or_lower_plan_is_refused_while_active_but_upgrade_works(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    buy(&pool, &svc, user, "pro", 10_001).await;

    for slug in ["pro", "basic"] {
        let err = svc.create_order(user, true, slug).await.err().unwrap();
        assert!(matches!(err, DatabaseError::ExistedDataError(_)), "{slug}");
    }

    buy(&pool, &svc, user, "ultra", 10_002).await;
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM subscriptions WHERE status = 'active'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM subscriptions WHERE status = 'canceled'"
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn renewal_is_allowed_near_the_end_and_extends_from_it(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    buy(&pool, &svc, user, "basic", 11_001).await;
    sqlx::query(
        "UPDATE subscriptions SET current_period_end = NOW() + interval '3 days' WHERE user_id = $1",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let end_before: DateTime<Utc> =
        sqlx::query_scalar("SELECT current_period_end FROM subscriptions WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();

    buy(&pool, &svc, user, "basic", 11_002).await;
    let end_after: DateTime<Utc> = sqlx::query_scalar(
        "SELECT current_period_end FROM subscriptions WHERE user_id = $1 AND status = 'active'",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(end_after - end_before, Duration::days(30));
}

#[sqlx::test(migrations = "../../migrations")]
async fn orders_belong_to_their_owner(pool: PgPool) {
    let svc = billing(&pool);
    let owner = make_user(&pool, true).await;
    let other = make_user(&pool, true).await;
    let order = svc.create_order(owner, true, "basic").await.unwrap().order;

    assert!(svc.get_order(owner, order.id).await.is_ok());
    assert!(matches!(
        svc.get_order(other, order.id).await,
        Err(DatabaseError::ValueNotFound(_))
    ));
    assert!(matches!(
        svc.order_details(other, order.id).await,
        Err(DatabaseError::ValueNotFound(_))
    ));
    assert!(svc.cancel_order(other, order.id).await.is_err());
    assert!(svc.check_order(other, order.id).await.is_err());
    assert!(svc.list_orders(other).await.unwrap().is_empty());
    assert_eq!(status_of(&pool, order.id).await, "pending");

    let canceled = svc.cancel_order(owner, order.id).await.unwrap();
    assert_eq!(canceled.status, "canceled");
    // A canceled order frees the user to start a new one.
    let fresh = svc.create_order(owner, true, "basic").await.unwrap().order;
    assert_ne!(fresh.id, order.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sweeper_expires_orders_and_subscriptions(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    buy(&pool, &svc, user, "basic", 12_001).await;
    let stale_user = make_user(&pool, true).await;
    let stale = svc
        .create_order(stale_user, true, "pro")
        .await
        .unwrap()
        .order;
    sqlx::query("UPDATE payment_orders SET expires_at = NOW() - interval '1 minute' WHERE id = $1")
        .bind(stale.id)
        .execute(&pool)
        .await
        .unwrap();
    // The API reports it as expired even before the sweeper runs.
    assert_eq!(
        svc.get_order(stale_user, stale.id).await.unwrap().status,
        "expired"
    );

    sqlx::query("UPDATE subscriptions SET current_period_start = NOW() - interval '31 days', current_period_end = NOW() - interval '1 second' WHERE user_id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let (orders, subs) = svc.sweep().await.unwrap();
    assert_eq!((orders, subs), (1, 1));
    assert_eq!(status_of(&pool, stale.id).await, "expired");
    assert_eq!(active_plan_slug(&pool, user).await, None);
    assert_eq!(svc.sweep().await.unwrap(), (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn check_order_settles_a_transfer_the_webhook_missed(pool: PgPool) {
    let user = make_user(&pool, true).await;
    // Create the order first so the fake provider can return its code.
    let probe = billing(&pool);
    let order = probe.create_order(user, true, "basic").await.unwrap().order;
    let transfer = Transfer {
        id: 13_001,
        incoming: true,
        amount_vnd: 99_000,
        content: format!("chuyen khoan {}", order.payment_code),
        code: None,
        raw: json!({"id": 13001}),
    };
    let svc = billing_with(&pool, vec![transfer]);

    let checked = svc.check_order(user, order.id).await.unwrap();
    assert_eq!(checked.status, "paid");
    assert_eq!(
        active_plan_slug(&pool, user).await.as_deref(),
        Some("basic")
    );
    let source: String =
        sqlx::query_scalar("SELECT source FROM payment_events WHERE sepay_transaction_id = 13001")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source, "reconcile");
    // The webhook arriving afterwards is a harmless duplicate.
    let late = deliver(&svc, 13_001, 99_000, &order.payment_code)
        .await
        .unwrap();
    assert!(matches!(late, SettleOutcome::Duplicate), "{late:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn check_order_is_rate_limited_per_order(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let order = svc.create_order(user, true, "basic").await.unwrap().order;
    svc.check_order(user, order.id).await.unwrap();
    assert!(matches!(
        svc.check_order(user, order.id).await,
        Err(DatabaseError::RateLimited(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn billing_is_disabled_without_configuration(pool: PgPool) {
    let repo: DynBillingRepository = Arc::new(PgBillingRepo::new(pool.clone()));
    let svc = BillingServices::new(repo, None, None);
    let user = make_user(&pool, true).await;
    assert!(!svc.is_enabled());
    assert!(matches!(
        svc.create_order(user, true, "basic").await,
        Err(DatabaseError::BillingDisabled)
    ));
    assert!(matches!(
        svc.handle_webhook(Some(&auth()), b"{}").await,
        Err(DatabaseError::BillingDisabled)
    ));
    // Plans and the free-tier summary still work.
    assert_eq!(svc.plans().await.unwrap().len(), 3);
    assert!(svc.subscription(user).await.unwrap().plan.is_none());
}

// ───────────────────────── gateway limits ─────────────────────────

async fn api_and_key(
    pool: &PgPool,
    user: Uuid,
    free_quota: i32,
    free_rate: i32,
) -> (wog_infras::models::Api, ApiKey) {
    let slug = format!("api-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let api: wog_infras::models::Api = sqlx::query_as(
        "INSERT INTO apis (slug, name, description, provider, category, gateway_path, upstream_method,
                           params, rate_limit_per_minute, monthly_quota)
         VALUES ($1, $1, 'd', 'p', 'c', $2, 'm', '[]'::jsonb, $3, $4) RETURNING *",
    )
    .bind(&slug)
    .bind(format!("/g/{slug}"))
    .bind(free_rate)
    .bind(free_quota)
    .fetch_one(pool)
    .await
    .unwrap();
    let key: ApiKey = sqlx::query_as(
        "INSERT INTO api_keys (user_id, name, key_prefix, key_hash)
         VALUES ($1, 'k', 'wog_test', $2) RETURNING *",
    )
    .bind(user)
    .bind(Uuid::new_v4().simple().to_string() + &Uuid::new_v4().simple().to_string())
    .fetch_one(pool)
    .await
    .unwrap();
    (api, key)
}

async fn use_requests(pool: &PgPool, key: &ApiKey, api_id: Uuid, n: i64) {
    sqlx::query(
        "INSERT INTO api_usage (api_key_id, user_id, api_id, status_code, latency_ms)
         SELECT $1, $2, $3, 200, 5 FROM generate_series(1, $4)",
    )
    .bind(key.id)
    .bind(key.user_id)
    .bind(api_id)
    .bind(n)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn free_tier_uses_the_apis_own_quota(pool: PgPool) {
    let user = make_user(&pool, true).await;
    let (api, key) = api_and_key(&pool, user, 5, 60).await;
    let gw = gateway(&pool);
    use_requests(&pool, &key, api.id, 4).await;
    assert!(gw.authorize(&key, &api).await.is_ok());
    use_requests(&pool, &key, api.id, 1).await;
    assert!(matches!(
        gw.authorize(&key, &api).await,
        Err(DatabaseError::RateLimited(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn free_tier_uses_the_apis_own_rate_limit(pool: PgPool) {
    let user = make_user(&pool, true).await;
    let (api, key) = api_and_key(&pool, user, 1_000_000, 3).await;
    let gw = gateway(&pool);
    for _ in 0..3 {
        gw.authorize(&key, &api).await.unwrap();
    }
    assert!(matches!(
        gw.authorize(&key, &api).await,
        Err(DatabaseError::RateLimited(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn paid_plan_quota_replaces_the_per_api_quota(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    // The API's own quota is tiny, the plan's (10,000) is not.
    let (api, key) = api_and_key(&pool, user, 2, 60).await;
    let gw = gateway(&pool);
    use_requests(&pool, &key, api.id, 2).await;
    assert!(
        gw.authorize(&key, &api).await.is_err(),
        "free quota exhausted"
    );

    buy(&pool, &svc, user, "basic", 14_001).await;
    assert!(gw.authorize(&key, &api).await.is_ok(), "plan pool applies");

    // The pool counts from the period start: the 2 requests made before the
    // purchase are not part of it.
    use_requests(&pool, &key, api.id, 9_999).await;
    assert!(gw.authorize(&key, &api).await.is_ok());
    use_requests(&pool, &key, api.id, 1).await;
    assert!(matches!(
        gw.authorize(&key, &api).await,
        Err(DatabaseError::RateLimited(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn paid_plan_rate_limit_follows_the_plan(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    // Free rate limit would allow only 1 request a minute.
    let (api, key) = api_and_key(&pool, user, 1_000_000, 1).await;
    let gw = gateway(&pool);
    buy(&pool, &svc, user, "basic", 15_001).await; // 30 / minute
    for _ in 0..30 {
        gw.authorize(&key, &api).await.unwrap();
    }
    assert!(matches!(
        gw.authorize(&key, &api).await,
        Err(DatabaseError::RateLimited(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_subscription_falls_back_to_free_at_request_time(pool: PgPool) {
    let svc = billing(&pool);
    let user = make_user(&pool, true).await;
    let (api, key) = api_and_key(&pool, user, 2, 60).await;
    let gw = gateway(&pool);
    buy(&pool, &svc, user, "basic", 16_001).await;
    use_requests(&pool, &key, api.id, 5).await;
    assert!(gw.authorize(&key, &api).await.is_ok());

    // Period ended, sweeper has NOT run yet.
    sqlx::query("UPDATE subscriptions SET current_period_start = NOW() - interval '31 days', current_period_end = NOW() - interval '1 second' WHERE user_id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        matches!(
            gw.authorize(&key, &api).await,
            Err(DatabaseError::RateLimited(_))
        ),
        "free quota of 2 applies again"
    );
    assert!(svc.subscription(user).await.unwrap().plan.is_none());
}
