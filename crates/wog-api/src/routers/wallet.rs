use axum::{Json, extract::State};
use wog_config::wallet::dto::{TransferCreditsRequest, TransferCreditsResponse, WalletResponse};
use wog_middleware::{AppState, AuthClaims};

use crate::errors::{RestApiError, RestApiResponseError};

#[utoipa::path(
    get,
    path = "/api/v1/wallet",
    tag = "Wallet",
    responses(
        (status = 200, description = "Get wallet balance", body = WalletResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_wallet(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<WalletResponse>, RestApiError> {
    let wallet = state.wallet_services.get_wallet(claims.sub).await?;
    Ok(Json(wallet.into()))
}

/// Send credits directly to another user's wallet -- a purely internal
/// ledger move between two accounts. This is the only way credits move
/// between accounts.
#[utoipa::path(
    post,
    path = "/api/v1/wallet/transfer",
    tag = "Wallet",
    request_body(content = TransferCreditsRequest, content_type = "application/json"),
    responses(
        (status = 200, description = "Credits transferred", body = TransferCreditsResponse),
        (status = 400, description = "Invalid recipient or amount", body = RestApiResponseError),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "No user found with that username or email", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn transfer_credits(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Json(payload): Json<TransferCreditsRequest>,
) -> Result<Json<TransferCreditsResponse>, RestApiError> {
    let (wallet, recipient) = state
        .wallet_services
        .transfer_credits(claims.sub, &payload.recipient, payload.amount)
        .await?;

    Ok(Json(TransferCreditsResponse {
        wallet: wallet.into(),
        recipient_username: recipient.username,
    }))
}
