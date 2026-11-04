use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use wog_infras::{
    models::{PaymentEvent, PaymentOrder, Plan},
    services::billing::{OrderDetails, SubscriptionSummary},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanResponse {
    pub slug: String,
    pub name: String,
    /// Integer VND.
    pub price_vnd: i32,
    pub period_days: i32,
    /// Requests per billing period across all APIs.
    pub monthly_quota: i32,
    pub rate_limit_per_minute: i32,
    pub features: Vec<String>,
    pub sort_order: i32,
}

impl From<Plan> for PlanResponse {
    fn from(value: Plan) -> Self {
        Self {
            slug: value.slug,
            name: value.name,
            price_vnd: value.price_vnd,
            period_days: value.period_days,
            monthly_quota: value.monthly_quota,
            rate_limit_per_minute: value.rate_limit_per_minute,
            features: value.features.0,
            sort_order: value.sort_order,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubscriptionPlanResponse {
    pub slug: String,
    pub name: String,
    pub monthly_quota: i32,
    pub rate_limit_per_minute: i32,
    pub current_period_start: DateTime<Utc>,
    pub current_period_end: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UsageSummaryResponse {
    /// Billable requests since `period_start`.
    pub used: i64,
    /// The plan's request pool; absent on the free tier, where each API has
    /// its own quota.
    pub quota: Option<i64>,
    pub period_start: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubscriptionResponse {
    /// `active` with a paid plan, `free` otherwise.
    pub status: String,
    pub plan: Option<SubscriptionPlanResponse>,
    pub usage: UsageSummaryResponse,
}

impl From<SubscriptionSummary> for SubscriptionResponse {
    fn from(value: SubscriptionSummary) -> Self {
        Self {
            status: if value.plan.is_some() {
                "active"
            } else {
                "free"
            }
            .into(),
            plan: value.plan.map(|p| SubscriptionPlanResponse {
                slug: p.plan_slug,
                name: p.plan_name,
                monthly_quota: p.monthly_quota,
                rate_limit_per_minute: p.rate_limit_per_minute,
                current_period_start: p.current_period_start,
                current_period_end: p.current_period_end,
            }),
            usage: UsageSummaryResponse {
                used: value.usage.used,
                quota: value.usage.quota,
                period_start: value.usage.period_start,
            },
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOrderRequest {
    /// Plan slug: `basic`, `pro` or `ultra`.
    pub plan: String,
}

/// An order as listed in the history.
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderResponse {
    pub id: Uuid,
    pub plan: String,
    pub plan_name: String,
    pub amount_vnd: i32,
    /// `pending`, `paid`, `expired`, `canceled` or `failed`.
    pub status: String,
    pub payment_code: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<PaymentOrder> for OrderResponse {
    fn from(value: PaymentOrder) -> Self {
        Self {
            id: value.id,
            plan: value.plan_slug,
            plan_name: value.plan_name,
            amount_vnd: value.amount_vnd,
            status: value.status,
            payment_code: value.payment_code,
            paid_at: value.paid_at,
            expires_at: value.expires_at,
            created_at: value.created_at,
        }
    }
}

/// An order plus everything needed to pay it.
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderPaymentResponse {
    pub id: Uuid,
    pub plan: String,
    pub plan_name: String,
    pub amount_vnd: i32,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    /// Exactly what to type as the transfer content.
    pub transfer_content: String,
    pub bank_name: String,
    pub account_number: String,
    pub account_holder: String,
    /// QR image that pre-fills the amount and the content.
    pub qr_url: String,
}

impl From<OrderDetails> for OrderPaymentResponse {
    fn from(value: OrderDetails) -> Self {
        Self {
            id: value.order.id,
            plan: value.order.plan_slug,
            plan_name: value.order.plan_name,
            amount_vnd: value.order.amount_vnd,
            status: value.order.status,
            expires_at: value.order.expires_at,
            created_at: value.order.created_at,
            transfer_content: value.transfer_content,
            bank_name: value.bank.bank_name,
            account_number: value.bank.account_number,
            account_holder: value.bank.account_holder,
            qr_url: value.qr_url,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderStatusResponse {
    pub id: Uuid,
    pub status: String,
    pub paid_at: Option<DateTime<Utc>>,
}

impl From<PaymentOrder> for OrderStatusResponse {
    fn from(value: PaymentOrder) -> Self {
        Self {
            id: value.id,
            status: value.status,
            paid_at: value.paid_at,
        }
    }
}

// ── Admin ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, IntoParams)]
pub struct PaymentsQuery {
    /// Orders: filter by status. Events: filter by outcome.
    pub filter: Option<String>,
    /// Events only: show just the ones that need a person to look.
    pub needs_review: Option<bool>,
    pub page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOrderResponse {
    #[serde(flatten)]
    pub order: OrderResponse,
    pub username: Option<String>,
    pub sepay_transaction_id: Option<i64>,
    pub paid_amount_vnd: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOrdersResponse {
    pub orders: Vec<AdminOrderResponse>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

impl AdminOrdersResponse {
    pub fn new(rows: Vec<PaymentOrder>, page: i64, per_page: i64) -> Self {
        let total = rows.first().map_or(0, |r| r.total);
        Self {
            orders: rows
                .into_iter()
                .map(|r| {
                    let username = r.username.clone();
                    let sepay_transaction_id = r.sepay_transaction_id;
                    let paid_amount_vnd = r.paid_amount_vnd;
                    AdminOrderResponse {
                        order: r.into(),
                        username,
                        sepay_transaction_id,
                        paid_amount_vnd,
                    }
                })
                .collect(),
            total,
            page,
            per_page,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaymentEventResponse {
    pub id: Uuid,
    /// `webhook` or `reconcile`.
    pub source: String,
    pub sepay_transaction_id: Option<i64>,
    pub transfer_type: Option<String>,
    pub amount_vnd: Option<i64>,
    pub payment_code: Option<String>,
    pub content: Option<String>,
    pub matched_order_id: Option<Uuid>,
    pub outcome: String,
    pub note: Option<String>,
    pub needs_review: bool,
    pub created_at: DateTime<Utc>,
}

impl From<PaymentEvent> for PaymentEventResponse {
    fn from(value: PaymentEvent) -> Self {
        Self {
            id: value.id,
            source: value.source,
            sepay_transaction_id: value.sepay_transaction_id,
            transfer_type: value.transfer_type,
            amount_vnd: value.amount_vnd,
            payment_code: value.payment_code,
            content: value.content,
            matched_order_id: value.matched_order_id,
            outcome: value.outcome,
            note: value.note,
            needs_review: value.needs_review,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminEventsResponse {
    pub events: Vec<PaymentEventResponse>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

impl AdminEventsResponse {
    pub fn new(rows: Vec<PaymentEvent>, page: i64, per_page: i64) -> Self {
        let total = rows.first().map_or(0, |r| r.total);
        Self {
            events: rows.into_iter().map(Into::into).collect(),
            total,
            page,
            per_page,
        }
    }
}
