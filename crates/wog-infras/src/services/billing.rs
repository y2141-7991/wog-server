use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use payment_client::{BankDetails, PaymentProvider};
use uuid::Uuid;

use crate::{
    billing::{
        BillingConfig, RENEWAL_WINDOW_DAYS, Transfer, extract_payment_codes, generate_payment_code,
        parse_webhook, webhook_key_matches,
    },
    errors::DatabaseError,
    models::{ActiveSubscription, PaymentEvent, PaymentOrder, Plan, SettleOutcome},
    repos::DynBillingRepository,
};

const MAX_CODE_ATTEMPTS: usize = 5;
const ORDER_HISTORY_LIMIT: i64 = 50;
/// A customer may press "check now" at most this often per order.
const CHECK_COOLDOWN: StdDuration = StdDuration::from_secs(5);
/// Allowance for clock skew between us, the bank and SePay when looking back
/// for transfers made after an order was created.
const RECONCILE_SLACK: Duration = Duration::minutes(10);

/// Everything the client needs to show a payment screen.
pub struct OrderDetails {
    pub order: PaymentOrder,
    pub bank: BankDetails,
    pub qr_url: String,
    /// What the customer must type as the transfer content.
    pub transfer_content: String,
}

pub struct UsageSummary {
    pub used: i64,
    /// `None` on the free tier, where each API has its own quota.
    pub quota: Option<i64>,
    pub period_start: DateTime<Utc>,
}

pub struct SubscriptionSummary {
    /// `None` on the free tier.
    pub plan: Option<ActiveSubscription>,
    pub usage: UsageSummary,
}

#[derive(Clone)]
pub struct BillingServices {
    repo: DynBillingRepository,
    provider: Option<Arc<dyn PaymentProvider>>,
    config: Option<BillingConfig>,
    last_checks: Arc<Mutex<HashMap<Uuid, Instant>>>,
}

fn start_of_utc_month(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .expect("the first of a month is a valid time")
}

/// A pending order past its expiry is reported as expired even before the
/// sweeper has marked it, so the customer never sees a dead countdown.
fn present(mut order: PaymentOrder) -> PaymentOrder {
    if order.status == "pending" && order.expires_at <= Utc::now() {
        order.status = "expired".into();
    }
    order
}

impl BillingServices {
    pub fn new(
        repo: DynBillingRepository,
        provider: Option<Arc<dyn PaymentProvider>>,
        config: Option<BillingConfig>,
    ) -> Self {
        Self {
            repo,
            provider,
            config,
            last_checks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_some() && self.provider.is_some()
    }

    fn enabled(&self) -> Result<(&BillingConfig, &Arc<dyn PaymentProvider>), DatabaseError> {
        match (&self.config, &self.provider) {
            (Some(config), Some(provider)) => Ok((config, provider)),
            _ => Err(DatabaseError::BillingDisabled),
        }
    }

    pub async fn plans(&self) -> Result<Vec<Plan>, DatabaseError> {
        self.repo.list_plans().await
    }

    pub async fn subscription(&self, user_id: Uuid) -> Result<SubscriptionSummary, DatabaseError> {
        match self.repo.find_active_subscription(user_id).await? {
            Some(plan) => {
                let used = self
                    .repo
                    .count_usage_since(user_id, plan.current_period_start)
                    .await?;
                Ok(SubscriptionSummary {
                    usage: UsageSummary {
                        used,
                        quota: Some(i64::from(plan.monthly_quota)),
                        period_start: plan.current_period_start,
                    },
                    plan: Some(plan),
                })
            }
            None => {
                let since = start_of_utc_month(Utc::now());
                Ok(SubscriptionSummary {
                    plan: None,
                    usage: UsageSummary {
                        used: self.repo.count_usage_since(user_id, since).await?,
                        quota: None,
                        period_start: since,
                    },
                })
            }
        }
    }

    fn details(&self, order: PaymentOrder) -> Result<OrderDetails, DatabaseError> {
        let (_, provider) = self.enabled()?;
        Ok(OrderDetails {
            qr_url: provider.qr_url(i64::from(order.amount_vnd), &order.payment_code),
            transfer_content: order.payment_code.clone(),
            bank: provider.bank_details().clone(),
            order: present(order),
        })
    }

    /// Start paying for a plan. Returns the user's existing pending order for
    /// that plan if there is one.
    ///
    /// A plan of the same tier can only be bought again in the last
    /// `RENEWAL_WINDOW_DAYS` of the running period (a renewal, which extends
    /// from the period end); a lower tier is refused; a higher tier is an
    /// upgrade that starts at payment time with no refund of the old plan.
    pub async fn create_order(
        &self,
        user_id: Uuid,
        email_verified: bool,
        plan_slug: &str,
    ) -> Result<OrderDetails, DatabaseError> {
        let (config, _) = self.enabled()?;
        if !email_verified {
            return Err(DatabaseError::Forbidden(
                "Verify your email before buying a plan".into(),
            ));
        }
        let plan = self
            .repo
            .find_plan(plan_slug)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("Plan not found".into()))?;

        if let Some(current) = self.repo.find_active_subscription(user_id).await? {
            if current.plan_id == plan.id {
                let left = current.current_period_end - Utc::now();
                if left > Duration::days(RENEWAL_WINDOW_DAYS) {
                    return Err(DatabaseError::ExistedDataError(format!(
                        "You already have {} until {}. You can renew it during its last {} days.",
                        current.plan_name,
                        current.current_period_end.format("%Y-%m-%d"),
                        RENEWAL_WINDOW_DAYS
                    )));
                }
            } else if current.plan_sort_order > plan.sort_order {
                return Err(DatabaseError::ExistedDataError(format!(
                    "You already have a higher plan ({})",
                    current.plan_name
                )));
            }
        }

        self.repo.expire_user_pending(user_id).await?;
        if let Some(pending) = self.repo.find_pending_order(user_id, plan.id).await? {
            return self.details(pending);
        }

        let expires_at = Utc::now() + config.order_ttl;
        for _ in 0..MAX_CODE_ATTEMPTS {
            let code = generate_payment_code(&config.code_prefix);
            match self
                .repo
                .create_order(user_id, &plan, &code, expires_at)
                .await
            {
                Ok(order) => return self.details(order),
                Err(DatabaseError::UniqueViolation) => {
                    // Either the code is taken (try another) or a parallel
                    // request just created this user's pending order.
                    if let Some(pending) = self.repo.find_pending_order(user_id, plan.id).await? {
                        return self.details(pending);
                    }
                }
                Err(other) => return Err(other),
            }
        }
        Err(DatabaseError::Others(
            "could not generate a unique payment code".into(),
        ))
    }

    /// An order the user owns. Someone else's order is reported as missing.
    pub async fn get_order(
        &self,
        user_id: Uuid,
        order_id: Uuid,
    ) -> Result<PaymentOrder, DatabaseError> {
        self.repo
            .find_order(order_id)
            .await?
            .filter(|order| order.user_id == user_id)
            .map(present)
            .ok_or_else(|| DatabaseError::ValueNotFound("Order not found".into()))
    }

    pub async fn order_details(
        &self,
        user_id: Uuid,
        order_id: Uuid,
    ) -> Result<OrderDetails, DatabaseError> {
        let order = self.get_order(user_id, order_id).await?;
        self.details(order)
    }

    pub async fn list_orders(&self, user_id: Uuid) -> Result<Vec<PaymentOrder>, DatabaseError> {
        Ok(self
            .repo
            .list_orders(user_id, ORDER_HISTORY_LIMIT)
            .await?
            .into_iter()
            .map(present)
            .collect())
    }

    pub async fn cancel_order(
        &self,
        user_id: Uuid,
        order_id: Uuid,
    ) -> Result<PaymentOrder, DatabaseError> {
        let mut order = self.repo.cancel_order(user_id, order_id).await?;
        order.status = "canceled".into();
        Ok(order)
    }

    /// "I've paid, check now": ask SePay for recent transfers and settle any
    /// that match, through the same path as the webhook. Rate limited per order.
    pub async fn check_order(
        &self,
        user_id: Uuid,
        order_id: Uuid,
    ) -> Result<PaymentOrder, DatabaseError> {
        let (_, provider) = self.enabled()?;
        let order = self.get_order(user_id, order_id).await?;
        if order.status != "pending" && order.status != "expired" {
            return Ok(order);
        }
        if !provider.can_list_transfers() {
            // No API token: the webhook is the only way a payment arrives, so
            // "check now" just reports the order as it stands.
            return Ok(order);
        }
        {
            let mut last = self.last_checks.lock().expect("check lock poisoned");
            let now = Instant::now();
            last.retain(|_, at| now.duration_since(*at) < CHECK_COOLDOWN);
            if last.contains_key(&order_id) {
                return Err(DatabaseError::RateLimited(
                    "Please wait a few seconds before checking again".into(),
                ));
            }
            last.insert(order_id, now);
        }
        self.reconcile().await?;
        self.get_order(user_id, order_id).await
    }

    /// Settle any transfers SePay lists that we have not processed yet,
    /// covering webhooks that never arrived. Returns how many were activated.
    pub async fn reconcile(&self) -> Result<usize, DatabaseError> {
        let (_, provider) = self.enabled()?;
        if !provider.can_list_transfers() {
            return Ok(0);
        }
        let Some(oldest) = self.repo.oldest_pending_order_time().await? else {
            return Ok(0);
        };
        let transfers = provider
            .recent_transfers(oldest - RECONCILE_SLACK)
            .await
            .map_err(|e| {
                tracing::warn!("SePay transactions lookup failed: {e}");
                DatabaseError::UpstreamFailure("Could not reach the payment provider".into())
            })?;

        let ids: Vec<i64> = transfers.iter().map(|t| t.id).collect();
        let seen = self.repo.seen_transaction_ids(&ids).await?;
        let mut activated = 0;
        for transfer in transfers.iter().filter(|t| !seen.contains(&t.id)) {
            if matches!(
                self.process_transfer("reconcile", transfer).await?,
                SettleOutcome::Activated { .. }
            ) {
                activated += 1;
            }
        }
        Ok(activated)
    }

    /// The webhook: check the key, parse the body, settle the transfer.
    /// Errors map to the HTTP status SePay should see: 401 for a bad key,
    /// 400 for a body we cannot read, 5xx (so SePay retries) for anything
    /// transient. Every other outcome is a success.
    pub async fn handle_webhook(
        &self,
        authorization: Option<&str>,
        body: &[u8],
    ) -> Result<SettleOutcome, DatabaseError> {
        let (config, _) = self.enabled()?;
        if !webhook_key_matches(authorization, &config.webhook_api_key) {
            return Err(DatabaseError::Unauthorized(
                "Invalid webhook credentials".into(),
            ));
        }
        let transfer = parse_webhook(body)
            .map_err(|_| DatabaseError::ValidationError("Unreadable webhook payload".into()))?;
        self.process_transfer("webhook", &transfer).await
    }

    /// The single path from a transfer to a subscription.
    pub async fn process_transfer(
        &self,
        source: &str,
        transfer: &Transfer,
    ) -> Result<SettleOutcome, DatabaseError> {
        let (config, _) = self.enabled()?;
        let codes = extract_payment_codes(
            &config.code_prefix,
            transfer.code.as_deref(),
            &transfer.content,
        );
        let outcome = self.repo.settle_transfer(source, transfer, &codes).await?;
        match &outcome {
            SettleOutcome::Activated {
                plan_slug,
                user_id,
                late,
                overpaid_vnd,
                ..
            } => tracing::info!(
                transaction = transfer.id, %user_id, plan = %plan_slug, late, overpaid_vnd,
                "Payment settled"
            ),
            SettleOutcome::Underpaid => tracing::warn!(
                transaction = transfer.id,
                amount = transfer.amount_vnd,
                "Underpaid transfer recorded for manual review"
            ),
            SettleOutcome::AlreadyPaid
            | SettleOutcome::CanceledOrder
            | SettleOutcome::NotApplied => tracing::warn!(
                transaction = transfer.id,
                amount = transfer.amount_vnd,
                ?outcome,
                "Transfer needs manual review"
            ),
            SettleOutcome::Unmatched if !codes.is_empty() => tracing::warn!(
                transaction = transfer.id,
                amount = transfer.amount_vnd,
                "Transfer carries a payment code that matches no order"
            ),
            _ => {}
        }
        Ok(outcome)
    }

    /// Housekeeping for the background task: expire stale orders and
    /// subscriptions. Returns how many of each.
    pub async fn sweep(&self) -> Result<(u64, u64), DatabaseError> {
        self.repo.expire_stale().await
    }

    pub async fn admin_orders(
        &self,
        status: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<PaymentOrder>, DatabaseError> {
        self.repo
            .admin_list_orders(status, per_page, (page.max(1) - 1) * per_page)
            .await
    }

    pub async fn admin_events(
        &self,
        outcome: Option<&str>,
        needs_review_only: bool,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<PaymentEvent>, DatabaseError> {
        self.repo
            .admin_list_events(
                outcome,
                needs_review_only,
                per_page,
                (page.max(1) - 1) * per_page,
            )
            .await
    }
}
