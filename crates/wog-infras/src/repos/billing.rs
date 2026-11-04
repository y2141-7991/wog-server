use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    billing::Transfer,
    errors::DatabaseError,
    models::{ActiveSubscription, PaymentEvent, PaymentOrder, Plan, SettleOutcome},
    repos::BillingRepository,
};

const ORDER_SELECT: &str = r#"
    SELECT o.*, p.slug AS plan_slug, p.name AS plan_name
    FROM payment_orders o
    JOIN plans p ON p.id = o.plan_id
"#;

const ACTIVE_SUBSCRIPTION_SELECT: &str = r#"
    SELECT s.id, s.user_id, s.plan_id, p.slug AS plan_slug, p.name AS plan_name,
           p.sort_order AS plan_sort_order, p.monthly_quota, p.rate_limit_per_minute,
           s.current_period_start, s.current_period_end
    FROM subscriptions s
    JOIN plans p ON p.id = s.plan_id
    WHERE s.user_id = $1 AND s.status = 'active' AND s.current_period_end > NOW()
"#;

/// The user's live subscription, if any. A row past `current_period_end` does
/// not count even before the sweeper marks it expired.
pub(crate) async fn find_active_subscription(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<ActiveSubscription>, DatabaseError> {
    Ok(
        sqlx::query_as::<_, ActiveSubscription>(ACTIVE_SUBSCRIPTION_SELECT)
            .bind(user_id)
            .fetch_optional(pool)
            .await?,
    )
}

/// Billable requests (upstream did not 5xx) across all APIs since `since`.
pub(crate) async fn count_usage_since(
    pool: &PgPool,
    user_id: Uuid,
    since: DateTime<Utc>,
) -> Result<i64, DatabaseError> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM api_usage WHERE user_id = $1 AND status_code < 500 AND created_at >= $2",
    )
    .bind(user_id)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

#[derive(Clone)]
pub struct PgBillingRepo {
    pub pg_pool: PgPool,
}

impl PgBillingRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

/// What to write to `payment_events` for one delivery.
struct EventRecord<'a> {
    source: &'a str,
    transfer: &'a Transfer,
    code: Option<&'a str>,
    order_id: Option<Uuid>,
    outcome: &'a str,
    note: Option<String>,
    needs_review: bool,
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    event: EventRecord<'_>,
) -> Result<(), DatabaseError> {
    sqlx::query(
        r#"
            INSERT INTO payment_events
                (source, sepay_transaction_id, transfer_type, amount_vnd, payment_code,
                 content, payload, matched_order_id, outcome, note, needs_review)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(event.source)
    .bind(event.transfer.id)
    .bind(if event.transfer.incoming { "in" } else { "out" })
    .bind(event.transfer.amount_vnd)
    .bind(event.code)
    .bind(&event.transfer.content)
    .bind(&event.transfer.raw)
    .bind(event.order_id)
    .bind(event.outcome)
    .bind(event.note)
    .bind(event.needs_review)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The user's active subscription row, locked, for the settle transaction.
#[derive(sqlx::FromRow)]
struct LockedSubscription {
    id: Uuid,
    plan_id: Uuid,
    plan_sort_order: i32,
    current_period_end: DateTime<Utc>,
}

#[async_trait]
impl BillingRepository for PgBillingRepo {
    async fn list_plans(&self) -> Result<Vec<Plan>, DatabaseError> {
        Ok(sqlx::query_as::<_, Plan>(
            "SELECT * FROM plans WHERE enabled ORDER BY sort_order, price_vnd",
        )
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn find_plan(&self, slug: &str) -> Result<Option<Plan>, DatabaseError> {
        Ok(
            sqlx::query_as::<_, Plan>("SELECT * FROM plans WHERE slug = $1 AND enabled")
                .bind(slug)
                .fetch_optional(&self.pg_pool)
                .await?,
        )
    }

    async fn find_active_subscription(
        &self,
        user_id: Uuid,
    ) -> Result<Option<ActiveSubscription>, DatabaseError> {
        find_active_subscription(&self.pg_pool, user_id).await
    }

    async fn count_usage_since(
        &self,
        user_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<i64, DatabaseError> {
        count_usage_since(&self.pg_pool, user_id, since).await
    }

    async fn expire_user_pending(&self, user_id: Uuid) -> Result<u64, DatabaseError> {
        Ok(sqlx::query(
            "UPDATE payment_orders SET status = 'expired' WHERE user_id = $1 AND status = 'pending' AND expires_at <= NOW()",
        )
        .bind(user_id)
        .execute(&self.pg_pool)
        .await?
        .rows_affected())
    }

    async fn find_pending_order(
        &self,
        user_id: Uuid,
        plan_id: Uuid,
    ) -> Result<Option<PaymentOrder>, DatabaseError> {
        Ok(sqlx::query_as::<_, PaymentOrder>(&format!(
            "{ORDER_SELECT} WHERE o.user_id = $1 AND o.plan_id = $2 AND o.status = 'pending' AND o.expires_at > NOW()"
        ))
        .bind(user_id)
        .bind(plan_id)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn create_order(
        &self,
        user_id: Uuid,
        plan: &Plan,
        payment_code: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<PaymentOrder, DatabaseError> {
        Ok(sqlx::query_as::<_, PaymentOrder>(
            r#"
                WITH o AS (
                    INSERT INTO payment_orders (user_id, plan_id, amount_vnd, payment_code, expires_at)
                    VALUES ($1, $2, $3, $4, $5)
                    RETURNING *
                )
                SELECT o.*, p.slug AS plan_slug, p.name AS plan_name
                FROM o JOIN plans p ON p.id = o.plan_id
            "#,
        )
        .bind(user_id)
        .bind(plan.id)
        .bind(plan.price_vnd)
        .bind(payment_code)
        .bind(expires_at)
        .fetch_one(&self.pg_pool)
        .await?)
    }

    async fn find_order(&self, order_id: Uuid) -> Result<Option<PaymentOrder>, DatabaseError> {
        Ok(
            sqlx::query_as::<_, PaymentOrder>(&format!("{ORDER_SELECT} WHERE o.id = $1"))
                .bind(order_id)
                .fetch_optional(&self.pg_pool)
                .await?,
        )
    }

    async fn list_orders(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<PaymentOrder>, DatabaseError> {
        Ok(sqlx::query_as::<_, PaymentOrder>(&format!(
            "{ORDER_SELECT} WHERE o.user_id = $1 ORDER BY o.created_at DESC LIMIT $2"
        ))
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn cancel_order(
        &self,
        user_id: Uuid,
        order_id: Uuid,
    ) -> Result<PaymentOrder, DatabaseError> {
        let canceled = sqlx::query(
            "UPDATE payment_orders SET status = 'canceled' WHERE id = $1 AND user_id = $2 AND status = 'pending'",
        )
        .bind(order_id)
        .bind(user_id)
        .execute(&self.pg_pool)
        .await?
        .rows_affected();

        let order = self
            .find_order(order_id)
            .await?
            .filter(|o| o.user_id == user_id)
            .ok_or_else(|| DatabaseError::ValueNotFound("Order not found".into()))?;
        if canceled == 0 {
            return Err(DatabaseError::ValidationError(
                "Only a pending order can be canceled".into(),
            ));
        }
        Ok(order)
    }

    async fn expire_stale(&self) -> Result<(u64, u64), DatabaseError> {
        let orders = sqlx::query(
            "UPDATE payment_orders SET status = 'expired' WHERE status = 'pending' AND expires_at <= NOW()",
        )
        .execute(&self.pg_pool)
        .await?
        .rows_affected();
        let subscriptions = sqlx::query(
            "UPDATE subscriptions SET status = 'expired', updated_at = NOW() WHERE status = 'active' AND current_period_end <= NOW()",
        )
        .execute(&self.pg_pool)
        .await?
        .rows_affected();
        Ok((orders, subscriptions))
    }

    async fn seen_transaction_ids(&self, ids: &[i64]) -> Result<Vec<i64>, DatabaseError> {
        let seen: Vec<(i64,)> = sqlx::query_as(
            "SELECT DISTINCT sepay_transaction_id FROM payment_events WHERE sepay_transaction_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pg_pool)
        .await?;
        Ok(seen.into_iter().map(|(id,)| id).collect())
    }

    async fn oldest_pending_order_time(&self) -> Result<Option<DateTime<Utc>>, DatabaseError> {
        let (oldest,): (Option<DateTime<Utc>>,) = sqlx::query_as(
            "SELECT MIN(created_at) FROM payment_orders WHERE status = 'pending' AND expires_at > NOW()",
        )
        .fetch_one(&self.pg_pool)
        .await?;
        Ok(oldest)
    }

    /// The one place a payment turns into a subscription, shared by the
    /// webhook and the reconciliation job. Everything happens in a single
    /// transaction holding a lock on the order row, so duplicate and
    /// concurrent deliveries of the same transfer settle exactly once.
    async fn settle_transfer(
        &self,
        source: &str,
        transfer: &Transfer,
        codes: &[String],
    ) -> Result<SettleOutcome, DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;
        let first_code = codes.first().map(String::as_str);

        if !transfer.incoming {
            insert_event(
                &mut tx,
                EventRecord {
                    source,
                    transfer,
                    code: first_code,
                    order_id: None,
                    outcome: "ignored",
                    note: Some("outgoing transfer".into()),
                    needs_review: false,
                },
            )
            .await?;
            tx.commit().await?;
            return Ok(SettleOutcome::Ignored);
        }

        let order = sqlx::query_as::<_, PaymentOrder>(&format!(
            "{ORDER_SELECT} WHERE o.payment_code = ANY($1) ORDER BY o.created_at LIMIT 1 FOR UPDATE OF o"
        ))
        .bind(codes)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(order) = order else {
            insert_event(
                &mut tx,
                EventRecord {
                    source,
                    transfer,
                    code: first_code,
                    order_id: None,
                    outcome: "unmatched",
                    note: Some(if codes.is_empty() {
                        "no payment code in the transfer".into()
                    } else {
                        "no order has this payment code".into()
                    }),
                    needs_review: true,
                },
            )
            .await?;
            tx.commit().await?;
            return Ok(SettleOutcome::Unmatched);
        };

        let event = |outcome: &'static str, note: Option<String>, needs_review: bool| EventRecord {
            source,
            transfer,
            code: Some(order.payment_code.as_str()),
            order_id: Some(order.id),
            outcome,
            note,
            needs_review,
        };

        // The same transfer delivered again (SePay retries and replays).
        if order.sepay_transaction_id == Some(transfer.id) {
            insert_event(&mut tx, event("duplicate", None, false)).await?;
            tx.commit().await?;
            return Ok(SettleOutcome::Duplicate);
        }
        if order.status == "paid" {
            insert_event(
                &mut tx,
                event(
                    "already_paid",
                    Some("a second transfer for an order that is already paid".into()),
                    true,
                ),
            )
            .await?;
            tx.commit().await?;
            return Ok(SettleOutcome::AlreadyPaid);
        }
        if order.status == "canceled" || order.status == "failed" {
            insert_event(
                &mut tx,
                event(
                    "canceled_order",
                    Some(format!("the order is {}", order.status)),
                    true,
                ),
            )
            .await?;
            tx.commit().await?;
            return Ok(SettleOutcome::CanceledOrder);
        }

        let price = i64::from(order.amount_vnd);
        if transfer.amount_vnd < price {
            insert_event(
                &mut tx,
                event(
                    "underpaid",
                    Some(format!("received {} of {} VND", transfer.amount_vnd, price)),
                    true,
                ),
            )
            .await?;
            tx.commit().await?;
            return Ok(SettleOutcome::Underpaid);
        }

        // Pending, or expired but not canceled: the customer did pay.
        let now = Utc::now();
        let late = order.status == "expired" || order.expires_at <= now;
        let overpaid_vnd = transfer.amount_vnd - price;

        sqlx::query(
            r#"
                UPDATE payment_orders
                SET status = 'paid', sepay_transaction_id = $2, paid_amount_vnd = $3, paid_at = $4
                WHERE id = $1
            "#,
        )
        .bind(order.id)
        .bind(transfer.id)
        .bind(transfer.amount_vnd)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        // One subscription change at a time per user, even for two different
        // orders paid at the same moment.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(order.user_id.to_string())
            .execute(&mut *tx)
            .await?;

        let (plan_period_days, plan_sort_order): (i32, i32) =
            sqlx::query_as("SELECT period_days, sort_order FROM plans WHERE id = $1")
                .bind(order.plan_id)
                .fetch_one(&mut *tx)
                .await?;
        let period = Duration::days(i64::from(plan_period_days));

        let mut current = sqlx::query_as::<_, LockedSubscription>(
            r#"
                SELECT s.id, s.plan_id, p.sort_order AS plan_sort_order, s.current_period_end
                FROM subscriptions s JOIN plans p ON p.id = s.plan_id
                WHERE s.user_id = $1 AND s.status = 'active'
                FOR UPDATE OF s
            "#,
        )
        .bind(order.user_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(stale) = current.as_ref().filter(|c| c.current_period_end <= now) {
            sqlx::query(
                "UPDATE subscriptions SET status = 'expired', updated_at = NOW() WHERE id = $1",
            )
            .bind(stale.id)
            .execute(&mut *tx)
            .await?;
            current = None;
        }

        let mut notes: Vec<String> = Vec::new();
        if late {
            notes.push("paid after the order expired".into());
        }
        if overpaid_vnd > 0 {
            notes.push(format!(
                "overpaid by {overpaid_vnd} VND, not refunded automatically"
            ));
        }

        let period_end = match current {
            Some(held) if held.plan_id == order.plan_id => {
                // Renewal: the new period starts where the old one ends.
                let end = held.current_period_end + period;
                sqlx::query("UPDATE subscriptions SET current_period_end = $2, updated_at = NOW() WHERE id = $1")
                    .bind(held.id)
                    .bind(end)
                    .execute(&mut *tx)
                    .await?;
                notes.push("renewed from the current period end".into());
                Some(end)
            }
            Some(held) if held.plan_sort_order > plan_sort_order => {
                // A higher plan is already running; do not downgrade it.
                None
            }
            other => {
                if let Some(held) = other {
                    // Upgrade: the old plan is replaced now, with no refund.
                    sqlx::query("UPDATE subscriptions SET status = 'canceled', updated_at = NOW() WHERE id = $1")
                        .bind(held.id)
                        .execute(&mut *tx)
                        .await?;
                    notes.push("upgrade: replaced the previous plan".into());
                }
                let end = now + period;
                sqlx::query(
                    r#"
                        INSERT INTO subscriptions (user_id, plan_id, current_period_start, current_period_end)
                        VALUES ($1, $2, $3, $4)
                    "#,
                )
                .bind(order.user_id)
                .bind(order.plan_id)
                .bind(now)
                .bind(end)
                .execute(&mut *tx)
                .await?;
                Some(end)
            }
        };

        let Some(period_end) = period_end else {
            insert_event(
                &mut tx,
                event(
                    "not_applied",
                    Some("paid, but the user already has a higher plan".into()),
                    true,
                ),
            )
            .await?;
            tx.commit().await?;
            return Ok(SettleOutcome::NotApplied);
        };

        insert_event(
            &mut tx,
            event(
                "activated",
                (!notes.is_empty()).then(|| notes.join("; ")),
                overpaid_vnd > 0,
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(SettleOutcome::Activated {
            order_id: order.id,
            user_id: order.user_id,
            plan_slug: order.plan_slug,
            period_end,
            late,
            overpaid_vnd,
        })
    }

    async fn admin_list_orders(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PaymentOrder>, DatabaseError> {
        Ok(sqlx::query_as::<_, PaymentOrder>(
            r#"
                SELECT o.*, p.slug AS plan_slug, p.name AS plan_name, u.username,
                       COUNT(*) OVER() AS total
                FROM payment_orders o
                JOIN plans p ON p.id = o.plan_id
                JOIN users u ON u.id = o.user_id
                WHERE ($1::text IS NULL OR o.status = $1)
                ORDER BY o.created_at DESC
                LIMIT $2 OFFSET $3
            "#,
        )
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn admin_list_events(
        &self,
        outcome: Option<&str>,
        needs_review_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PaymentEvent>, DatabaseError> {
        Ok(sqlx::query_as::<_, PaymentEvent>(
            r#"
                SELECT id, source, sepay_transaction_id, transfer_type, amount_vnd, payment_code,
                       content, matched_order_id, outcome, note, needs_review, created_at,
                       COUNT(*) OVER() AS total
                FROM payment_events
                WHERE ($1::text IS NULL OR outcome = $1)
                  AND (NOT $2 OR needs_review)
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
            "#,
        )
        .bind(outcome)
        .bind(needs_review_only)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pg_pool)
        .await?)
    }
}
