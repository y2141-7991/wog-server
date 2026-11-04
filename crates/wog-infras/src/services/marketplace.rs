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
/// How long the public catalog (with its 30-day usage metrics, which take a few
/// aggregate queries to compute) is served from memory before it is read again.
/// A catalog edit made in the database shows up within this time.
const CATALOG_TTL: Duration = Duration::from_secs(60);

/// Fixed one-minute window per (key, api): window start and requests in it.
type RateWindows = Arc<Mutex<HashMap<(Uuid, Uuid), (Instant, u32)>>>;

/// The catalog as of a moment ago, shared by every request.
type CatalogCache = Arc<Mutex<Option<(Instant, Vec<Api>)>>>;

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
    catalog: CatalogCache,
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
            catalog: Arc::new(Mutex::new(None)),
        }
    }

    fn cached_catalog(&self) -> Option<Vec<Api>> {
        let cache = self.catalog.lock().expect("catalog cache poisoned");
        match cache.as_ref() {
            Some((at, apis)) if at.elapsed() < CATALOG_TTL => Some(apis.clone()),
            _ => None,
        }
    }

    pub async fn list_apis(&self) -> Result<Vec<Api>, DatabaseError> {
        if let Some(apis) = self.cached_catalog() {
            return Ok(apis);
        }
        let apis = self.repo.list_apis().await?;
        *self.catalog.lock().expect("catalog cache poisoned") =
            Some((Instant::now(), apis.clone()));
        Ok(apis)
    }

    pub async fn get_api(&self, slug: &str) -> Result<Api, DatabaseError> {
        if let Some(api) = self
            .cached_catalog()
            .and_then(|apis| apis.into_iter().find(|a| a.slug == slug))
        {
            return Ok(api);
        }
        self.repo
            .find_api_by_slug(slug)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("API not found".into()))
    }

    /// Create a key. The plaintext is returned once here and never stored:
    /// only its SHA-256 is kept.
    pub async fn create_key(
        &self,
        user_id: Uuid,
        name: &str,
    ) -> Result<(ApiKey, String), DatabaseError> {
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

        let plaintext = format!("wog_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
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

    /// Enforce the rate limit and monthly quota that apply to this key's
    /// owner for this API.
    ///
    /// With a running subscription the plan's numbers apply: its rate limit
    /// per minute per key (across all APIs) and its quota as one pool of
    /// billable requests across all APIs, counted from the start of the
    /// current billing period. Without one (the free tier, which includes an
    /// expired subscription) each API's own `rate_limit_per_minute` and
    /// `monthly_quota` apply, the quota counted per API over the UTC month.
    pub async fn authorize(&self, key: &ApiKey, api: &Api) -> Result<(), DatabaseError> {
        if let Some(plan) = self.repo.find_active_subscription(key.user_id).await? {
            self.check_rate_limit(key.id, Uuid::nil(), plan.rate_limit_per_minute)?;
            let used = self
                .repo
                .count_usage_since(key.user_id, plan.current_period_start)
                .await?;
            if used >= i64::from(plan.monthly_quota) {
                return Err(DatabaseError::RateLimited(format!(
                    "Quota of {} requests for the {} plan is used up",
                    plan.monthly_quota, plan.plan_name
                )));
            }
            return Ok(());
        }

        self.check_rate_limit(key.id, api.id, api.rate_limit_per_minute)?;
        let used = self
            .repo
            .count_usage_this_month(key.user_id, api.id)
            .await?;
        if used >= i64::from(api.monthly_quota) {
            return Err(DatabaseError::RateLimited(format!(
                "Monthly quota of {} requests for {} is used up",
                api.monthly_quota, api.name
            )));
        }
        Ok(())
    }

    /// The gateway: authenticate the key, validate params, enforce the rate
    /// limit and quota (see `authorize`), forward to the upstream, and meter the call.
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
        self.authorize(&key, &api).await?;

        let started = Instant::now();
        let (status, body) = match self.crawler.call(&api.upstream_method, forwarded).await {
            Ok(body) => (200, body),
            Err(e) => match e.code() {
                // Not the customer's doing and not a crawl result: don't meter it.
                Code::Unavailable | Code::Unauthenticated | Code::FailedPrecondition => {
                    tracing::error!(code = ?e.code(), "Crawler unavailable: {}", e.message());
                    return Err(DatabaseError::UpstreamFailure(
                        "Upstream is unavailable".into(),
                    ));
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

    /// Fixed one-minute window per (key, scope): the scope is the API on the
    /// free tier and `Uuid::nil()` (all APIs) on a plan.
    fn check_rate_limit(
        &self,
        key_id: Uuid,
        scope: Uuid,
        limit_per_minute: i32,
    ) -> Result<(), DatabaseError> {
        let now = Instant::now();
        let mut windows = self.rate_windows.lock().expect("rate limit lock poisoned");
        windows.retain(|_, (start, _)| now.duration_since(*start) < RATE_WINDOW);
        let (_, count) = windows.entry((key_id, scope)).or_insert((now, 0));
        if *count >= limit_per_minute as u32 {
            return Err(DatabaseError::RateLimited(format!(
                "Rate limit of {limit_per_minute} requests per minute exceeded"
            )));
        }
        *count += 1;
        Ok(())
    }
}
