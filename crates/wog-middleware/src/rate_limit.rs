use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use serde_json::json;

#[derive(Clone)]
struct TokenBucket {
    remaining: u64,
    reset_at: Instant,
}

#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<DashMap<String, TokenBucket>>,
    max_requests: u64,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u64, window: Duration) -> Self {
        let limiter = Self {
            buckets: Arc::new(DashMap::new()),
            max_requests,
            window,
        };

        // Background task to evict expired entries
        let buckets = limiter.buckets.clone();
        let window = limiter.window;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(window).await;
                let now = Instant::now();
                buckets.retain(|_, bucket| bucket.reset_at > now);
            }
        });

        limiter
    }

    fn check(&self, key: &str) -> (u64, u64, Duration) {
        let now = Instant::now();
        let mut entry = self.buckets.entry(key.to_string()).or_insert(TokenBucket {
            remaining: self.max_requests,
            reset_at: now + self.window,
        });

        // Window expired — reset the bucket
        if now >= entry.reset_at {
            entry.remaining = self.max_requests;
            entry.reset_at = now + self.window;
        }

        let reset_in = entry.reset_at.duration_since(now);

        if entry.remaining > 0 {
            entry.remaining -= 1;
            (self.max_requests, entry.remaining, reset_in)
        } else {
            (self.max_requests, 0, reset_in)
        }
    }
}

fn extract_client_key(req: &Request) -> String {
    // Try X-Forwarded-For first (behind reverse proxy)
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = forwarded.to_str() {
            if let Some(ip) = val.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }

    // Try X-Real-Ip
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(val) = real_ip.to_str() {
            return val.trim().to_string();
        }
    }

    // Fallback to User-Agent (not ideal, but better than nothing)
    req.headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}

pub async fn rate_limit_middleware(
    axum::extract::Extension(limiter): axum::extract::Extension<RateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let key = extract_client_key(&req);
    let user_agent = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let (limit, remaining, reset_in) = limiter.check(&key);
    let reset_secs = reset_in.as_secs();

    if remaining == 0
        && limiter
            .buckets
            .get(&key)
            .map_or(false, |b| b.remaining == 0)
    {
        tracing::warn!(
            client = %key,
            user_agent = %user_agent,
            "Rate limit exceeded"
        );

        let body = json!({
            "error": true,
            "message": "Too many requests. Please try again later."
        });

        let mut response = (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response();
        let headers = response.headers_mut();
        headers.insert("X-RateLimit-Limit", HeaderValue::from(limit));
        headers.insert("X-RateLimit-Remaining", HeaderValue::from(0u64));
        headers.insert("X-RateLimit-Reset", HeaderValue::from(reset_secs));
        headers.insert("Retry-After", HeaderValue::from(reset_secs));
        return response;
    }

    tracing::debug!(
        client = %key,
        user_agent = %user_agent,
        remaining = remaining,
        "Request allowed"
    );

    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert("X-RateLimit-Limit", HeaderValue::from(limit));
    headers.insert("X-RateLimit-Remaining", HeaderValue::from(remaining));
    headers.insert("X-RateLimit-Reset", HeaderValue::from(reset_secs));

    response
}
