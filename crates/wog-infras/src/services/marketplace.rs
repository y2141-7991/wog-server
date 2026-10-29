use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tonic::Code;
use uuid::Uuid;

use crate::{
    crawler::Crawler,
    errors::DatabaseError,
    models::{Api, ApiKey, ApiUsageByApi, ApiUsageByDay},
    repos::DynMarketplaceRepository,
};

const MAX_ACTIVE_KEYS_PER_USER: usize = 10;
const MAX_PARAM_VALUE_LEN: usize = 200;
const RATE_WINDOW: Duration = Duration::from_secs(60);

/// Fixed one-minute window per (key, api): window start and requests in it.
type RateWindows = Arc<Mutex<HashMap<(Uuid, Uuid), (Instant, u32)>>>;

/// What the upstream answered, passed through to the API customer.
pub struct GatewayResponse {
    pub status: u16,
    pub body: Value,
}

#[derive(Clone)]
pub struct MarketplaceServices {
    repo: DynMarketplaceRepository,
    crawler: Crawler,
    rate_windows: RateWindows,
}

fn hash_key(plaintext: &str) -> String {
    format!("{:x}", Sha256::digest(plaintext.as_bytes()))
}

impl MarketplaceServices {
    pub fn new(repo: DynMarketplaceRepository, crawler: Crawler) -> Self {
        Self {
            repo,
            crawler,
            rate_windows: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn list_apis(&self) -> Result<Vec<Api>, DatabaseError> {
        self.repo.list_apis().await
    }

    pub async fn get_api(&self, slug: &str) -> Result<Api, DatabaseError> {
        self.repo
            .find_api_by_slug(slug)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("API not found".into()))
    }

    /// Create a key. The plaintext is returned once here and never stored:
    /// only its SHA-256 is kept.
    pub async fn create_key(&self, user_id: Uuid, name: &str) -> Result<(ApiKey, String), DatabaseError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(DatabaseError::ValidationError(
                "Key name must be 1-100 characters".into(),
            ));
        }
        let active = self
            .repo
            .list_keys(user_id)
            .await?
            .iter()
            .filter(|k| k.revoked_at.is_none())
            .count();
        if active >= MAX_ACTIVE_KEYS_PER_USER {
            return Err(DatabaseError::ValidationError(format!(
                "You can have at most {MAX_ACTIVE_KEYS_PER_USER} active keys; revoke one first"
            )));
        }

        let plaintext = format!(
            "wog_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let key = self
            .repo
            .create_key(user_id, name, &plaintext[..12], &hash_key(&plaintext))
            .await?;
        Ok((key, plaintext))
    }

    pub async fn list_keys(&self, user_id: Uuid) -> Result<Vec<ApiKey>, DatabaseError> {
        self.repo.list_keys(user_id).await
    }

    pub async fn revoke_key(&self, user_id: Uuid, key_id: Uuid) -> Result<(), DatabaseError> {
        self.repo.revoke_key(user_id, key_id).await
    }

    pub async fn usage(
        &self,
        user_id: Uuid,
        days: i32,
    ) -> Result<(Vec<ApiUsageByApi>, Vec<ApiUsageByDay>), DatabaseError> {
        let days = days.clamp(1, 90);
        Ok((
            self.repo.usage_by_api(user_id, days).await?,
            self.repo.usage_by_day(user_id, days).await?,
        ))
    }

    /// The gateway: authenticate the key, validate params, enforce the rate
    /// limit and monthly quota, forward to the upstream, and meter the call.
    pub async fn call_api(
        &self,
        plaintext_key: &str,
        gateway_path: &str,
        query: &HashMap<String, String>,
    ) -> Result<GatewayResponse, DatabaseError> {
        let key = self
            .repo
            .find_active_key_by_hash(&hash_key(plaintext_key))
            .await?
            .ok_or(DatabaseError::InvalidApiKey)?;
        let api = self
            .repo
            .find_api_by_gateway_path(gateway_path)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("API not found".into()))?;

        let forwarded = Self::validated_params(&api, query)?;
        self.check_rate_limit(&key, &api)?;

        let used = self.repo.count_usage_this_month(key.user_id, api.id).await?;
        if used >= i64::from(api.monthly_quota) {
            return Err(DatabaseError::RateLimited(format!(
                "Monthly quota of {} requests for {} is used up",
                api.monthly_quota, api.name
            )));
        }

        let started = Instant::now();
        let (status, body) = match self.crawler.call(&api.upstream_method, forwarded).await {
            Ok(body) => (200, body),
            Err(e) => match e.code() {
                // Not the customer's doing and not a crawl result: don't meter it.
                Code::Unavailable | Code::Unauthenticated | Code::FailedPrecondition => {
                    tracing::error!(code = ?e.code(), "Crawler unavailable: {}", e.message());
                    return Err(DatabaseError::UpstreamFailure("Upstream is unavailable".into()));
                }
                code => Self::crawl_failure(code, e.message()),
            },
        };
        let latency_ms = started.elapsed().as_millis().min(i32::MAX as u128) as i32;

        self.repo
            .record_usage(&key, api.id, status as i16, latency_ms)
            .await?;
        Ok(GatewayResponse { status, body })
    }

    /// The HTTP status and body a customer sees when the crawl itself failed.
    /// Only failures that are the customer's to know about show their message.
    fn crawl_failure(code: Code, message: &str) -> (u16, Value) {
        let (status, shown) = match code {
            Code::InvalidArgument => (400, true),
            Code::NotFound => (404, true),
            Code::PermissionDenied => (403, true),
            Code::Internal => (502, true),
            Code::DeadlineExceeded => (504, false),
            _ => (502, false),
        };
        let message = if shown { message } else { "Upstream error" };
        (status, json!({ "error": true, "message": message }))
    }

    /// Keep only the parameters the catalog declares, and require the
    /// mandatory ones.
    fn validated_params(
        api: &Api,
        query: &HashMap<String, String>,
    ) -> Result<Vec<(String, String)>, DatabaseError> {
        let mut forwarded = Vec::new();
        for param in api.params.0.iter() {
            match query.get(&param.name).map(|v| v.trim()) {
                Some(value) if !value.is_empty() => {
                    if value.len() > MAX_PARAM_VALUE_LEN {
                        return Err(DatabaseError::ValidationError(format!(
                            "Parameter '{}' is too long",
                            param.name
                        )));
                    }
                    forwarded.push((param.name.clone(), value.to_string()));
                }
                _ if param.required => {
                    return Err(DatabaseError::ValidationError(format!(
                        "Missing required parameter '{}'",
                        param.name
                    )));
                }
                _ => {}
            }
        }
        Ok(forwarded)
    }

    fn check_rate_limit(&self, key: &ApiKey, api: &Api) -> Result<(), DatabaseError> {
        let now = Instant::now();
        let mut windows = self.rate_windows.lock().expect("rate limit lock poisoned");
        windows.retain(|_, (start, _)| now.duration_since(*start) < RATE_WINDOW);
        let (_, count) = windows.entry((key.id, api.id)).or_insert((now, 0));
        if *count >= api.rate_limit_per_minute as u32 {
            return Err(DatabaseError::RateLimited(format!(
                "Rate limit of {} requests per minute exceeded",
                api.rate_limit_per_minute
            )));
        }
        *count += 1;
        Ok(())
    }
}
