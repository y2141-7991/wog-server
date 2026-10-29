use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use wog_infras::models::{Api, ApiKey, ApiParam, ApiUsageByApi, ApiUsageByDay};

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiParamResponse {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub required: bool,
    pub description: String,
    pub example: String,
}

impl From<ApiParam> for ApiParamResponse {
    fn from(value: ApiParam) -> Self {
        Self {
            name: value.name,
            kind: value.kind,
            required: value.required,
            description: value.description,
            example: value.example,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub provider: String,
    pub category: String,
    pub gateway_path: String,
    pub params: Vec<ApiParamResponse>,
    #[schema(value_type = Option<Object>)]
    pub example_response: Option<serde_json::Value>,
    pub rate_limit_per_minute: i32,
    pub monthly_quota: i32,
    pub updated_at: DateTime<Utc>,
    /// Average latency over the last 30 days; `None` until the API has traffic.
    pub avg_latency_ms: Option<i32>,
    /// Share of calls that did not fail upstream (0-100); `None` until traffic.
    pub success_rate: Option<f64>,
    pub total_calls: i64,
}

impl From<Api> for ApiResponse {
    fn from(value: Api) -> Self {
        Self {
            id: value.id,
            slug: value.slug,
            name: value.name,
            description: value.description,
            provider: value.provider,
            category: value.category,
            gateway_path: value.gateway_path,
            params: value.params.0.into_iter().map(Into::into).collect(),
            example_response: value.example_response.map(|j| j.0),
            rate_limit_per_minute: value.rate_limit_per_minute,
            monthly_quota: value.monthly_quota,
            updated_at: value.updated_at,
            avg_latency_ms: value.avg_latency_ms,
            success_rate: value.success_rate,
            total_calls: value.total_calls,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<ApiKey> for ApiKeyResponse {
    fn from(value: ApiKey) -> Self {
        Self {
            id: value.id,
            name: value.name,
            key_prefix: value.key_prefix,
            created_at: value.created_at,
            last_used_at: value.last_used_at,
            revoked_at: value.revoked_at,
        }
    }
}

/// The only response that ever contains the full key.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub key: ApiKeyResponse,
    pub secret: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct UsageQuery {
    /// Look-back window in days (1-90, default 30)
    pub days: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UsageByApiResponse {
    pub slug: String,
    pub name: String,
    pub monthly_quota: i32,
    pub requests: i64,
    pub errors: i64,
    pub used_this_month: i64,
}

impl From<ApiUsageByApi> for UsageByApiResponse {
    fn from(value: ApiUsageByApi) -> Self {
        Self {
            slug: value.slug,
            name: value.name,
            monthly_quota: value.monthly_quota,
            requests: value.requests,
            errors: value.errors,
            used_this_month: value.used_this_month,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UsageByDayResponse {
    pub day: NaiveDate,
    pub requests: i64,
}

impl From<ApiUsageByDay> for UsageByDayResponse {
    fn from(value: ApiUsageByDay) -> Self {
        Self {
            day: value.day,
            requests: value.requests,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UsageResponse {
    pub days: i32,
    pub by_api: Vec<UsageByApiResponse>,
    pub by_day: Vec<UsageByDayResponse>,
}
