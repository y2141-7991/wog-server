use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, header::AUTHORIZATION},
};
use serde_json::{Value, json};
use uuid::Uuid;
use wog_config::billing::dto::{
    CreateOrderRequest, OrderPaymentResponse, OrderResponse, OrderStatusResponse, PlanResponse,
    SubscriptionResponse,
};
use wog_middleware::{AppState, AuthClaims};

use crate::errors::{RestApiError, RestApiResponseError};

#[utoipa::path(
    get,
    path = "/api/v1/billing/plans",
    tag = "Billing",
    responses(
        (status = 200, description = "Plans that can be bought, cheapest first", body = Vec<PlanResponse>),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    )
)]
#[axum::debug_handler]
pub async fn list_plans(
    State(state): State<AppState>,
) -> Result<Json<Vec<PlanResponse>>, RestApiError> {
    let plans = state.billing_services.plans().await?;
    Ok(Json(plans.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/billing/subscription",
    tag = "Billing",
    responses(
        (status = 200, description = "Your current plan and usage in the current period", body = SubscriptionResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_subscription(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<SubscriptionResponse>, RestApiError> {
    Ok(Json(
        state
            .billing_services
            .subscription(claims.sub)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/billing/orders",
    tag = "Billing",
    request_body(content = CreateOrderRequest, content_type = "application/json"),
    responses(
        (status = 200, description = "A pending order (your existing one if there already is one for this plan)", body = OrderPaymentResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Email not verified", body = RestApiResponseError),
        (status = 404, description = "Unknown plan", body = RestApiResponseError),
        (status = 409, description = "You already have this plan or a higher one", body = RestApiResponseError),
        (status = 503, description = "Billing is not configured", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn create_order(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<OrderPaymentResponse>, RestApiError> {
    let user = state.user_services.get_user(claims.sub).await?;
    let details = state
        .billing_services
        .create_order(user.id, user.email_verified, payload.plan.trim())
        .await?;
    Ok(Json(details.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/billing/orders",
    tag = "Billing",
    responses(
        (status = 200, description = "Your orders, newest first", body = Vec<OrderResponse>),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn list_orders(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<Vec<OrderResponse>>, RestApiError> {
    let orders = state.billing_services.list_orders(claims.sub).await?;
    Ok(Json(orders.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/billing/orders/{id}",
    tag = "Billing",
    params(("id" = Uuid, Path, description = "Order ID")),
    responses(
        (status = 200, description = "One of your orders with its payment details", body = OrderPaymentResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "Order not found", body = RestApiResponseError),
        (status = 503, description = "Billing is not configured", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_order(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderPaymentResponse>, RestApiError> {
    Ok(Json(
        state
            .billing_services
            .order_details(claims.sub, id)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/billing/orders/{id}/status",
    tag = "Billing",
    params(("id" = Uuid, Path, description = "Order ID")),
    responses(
        (status = 200, description = "The order's status, for polling", body = OrderStatusResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "Order not found", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_order_status(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderStatusResponse>, RestApiError> {
    Ok(Json(
        state
            .billing_services
            .get_order(claims.sub, id)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/billing/orders/{id}/cancel",
    tag = "Billing",
    params(("id" = Uuid, Path, description = "Order ID")),
    responses(
        (status = 200, description = "The canceled order", body = OrderStatusResponse),
        (status = 400, description = "Only a pending order can be canceled", body = RestApiResponseError),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "Order not found", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn cancel_order(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderStatusResponse>, RestApiError> {
    Ok(Json(
        state
            .billing_services
            .cancel_order(claims.sub, id)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/billing/orders/{id}/check",
    tag = "Billing",
    params(("id" = Uuid, Path, description = "Order ID")),
    responses(
        (status = 200, description = "The order after asking the payment provider for new transfers", body = OrderStatusResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "Order not found", body = RestApiResponseError),
        (status = 429, description = "Checked too recently", body = RestApiResponseError),
        (status = 502, description = "The payment provider could not be reached", body = RestApiResponseError),
        (status = 503, description = "Billing is not configured", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn check_order(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderStatusResponse>, RestApiError> {
    Ok(Json(
        state
            .billing_services
            .check_order(claims.sub, id)
            .await?
            .into(),
    ))
}

/// SePay's payment notification. Authenticated by `Authorization: Apikey <key>`
/// instead of a session. Answers `{"success": true}` (the only body SePay
/// accepts as delivered) for every transfer it understood, whether or not it
/// matched an order, so SePay does not retry those; a bad key is 401 and a
/// database failure is 5xx so SePay retries.
#[utoipa::path(
    post,
    path = "/api/v1/webhooks/sepay",
    tag = "Billing",
    request_body(content = Value, content_type = "application/json"),
    responses(
        (status = 200, description = "Processed", body = Value),
        (status = 400, description = "Unreadable payload", body = RestApiResponseError),
        (status = 401, description = "Wrong or missing API key", body = RestApiResponseError),
        (status = 503, description = "Billing is not configured", body = RestApiResponseError),
    )
)]
pub async fn sepay_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, RestApiError> {
    let authorization = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    state
        .billing_services
        .handle_webhook(authorization, &body)
        .await?;
    Ok(Json(json!({ "success": true })))
}
