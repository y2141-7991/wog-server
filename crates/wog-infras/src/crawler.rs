//! gRPC client for the crawler service (social-track-pipeline's `tiktok_gateway`).

use std::time::Duration;

use serde_json::Value;
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    transport::{Channel, Endpoint},
};

use self::proto::{CallRequest, crawler_client::CrawlerClient};

pub mod proto {
    tonic::include_proto!("crawler.v1");
}

/// Crawls can page through TikTok with delays, so give them time.
const CALL_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Clone)]
pub struct Crawler {
    client: CrawlerClient<Channel>,
    secret: MetadataValue<Ascii>,
}

impl Crawler {
    /// Connects lazily, so the server can start before the crawler is up.
    pub fn new(url: &str, secret: &str) -> anyhow::Result<Self> {
        let channel = Endpoint::from_shared(url.to_string())?
            .timeout(CALL_TIMEOUT)
            .connect_lazy();
        Ok(Self {
            client: CrawlerClient::new(channel),
            secret: secret.parse()?,
        })
    }

    pub async fn call(
        &self,
        method: &str,
        params: Vec<(String, String)>,
    ) -> Result<Value, Status> {
        let mut request = Request::new(CallRequest {
            method: method.to_string(),
            params: params.into_iter().collect(),
        });
        request
            .metadata_mut()
            .insert("x-upstream-secret", self.secret.clone());
        let result = self.client.clone().call(request).await?.into_inner();
        Ok(struct_to_json(result))
    }
}

fn struct_to_json(s: prost_types::Struct) -> Value {
    Value::Object(s.fields.into_iter().map(|(k, v)| (k, value_to_json(v))).collect())
}

fn value_to_json(v: prost_types::Value) -> Value {
    use prost_types::value::Kind;
    match v.kind {
        None | Some(Kind::NullValue(_)) => Value::Null,
        // protobuf Struct numbers are doubles; hand whole numbers back as integers.
        Some(Kind::NumberValue(n)) if n.fract() == 0.0 && n.abs() < 9e15 => Value::from(n as i64),
        Some(Kind::NumberValue(n)) => serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number),
        Some(Kind::StringValue(s)) => Value::String(s),
        Some(Kind::BoolValue(b)) => Value::Bool(b),
        Some(Kind::StructValue(s)) => struct_to_json(s),
        Some(Kind::ListValue(l)) => Value::Array(l.values.into_iter().map(value_to_json).collect()),
    }
}
