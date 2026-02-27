use std::{sync::Arc, time::Duration};

use envconfig::Envconfig;
use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Envconfig, Clone)]
struct VietQRConfig {
    #[envconfig(from = "VIET_QR_CLIENT_ID")]
    client_id: Option<String>,
    #[envconfig(from = "VIET_QR_API_KEY")]
    api_key: Option<String>,
}

pub trait ClientStrategy: Send + Sync {
    fn build_headers(&self) -> Result<HeaderMap>;
    fn build_payload(&self) -> Value;
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionInformation {
    account_no: String,
    account_name: String,
    acq_id: String,
    add_info: String,
    amount: String,
    template: String,
}

impl TransactionInformation {
    fn new(
        account_no: &str,
        account_name: &str,
        acq_id: &str,
        add_info: &str,
        amount: &str,
    ) -> Self {
        Self {
            account_no: account_no.to_string(),
            account_name: account_name.to_string(),
            acq_id: acq_id.to_string(),
            add_info: add_info.to_string(),
            amount: amount.to_string(),
            template: "compact".to_string(),
        }
    }
}

pub struct VietQRClient {
    viet_qr_config: VietQRConfig,
    transaction_info: TransactionInformation,
}

impl ClientStrategy for VietQRClient {
    fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        let client_id = self
            .viet_qr_config
            .client_id
            .as_deref()
            .ok_or_else(|| ResponseError::ParseError("missing VIET_QR_CLIENT_ID".into()))?;
        headers.insert(
            "x-client-id",
            HeaderValue::from_str(client_id)
                .map_err(|e| ResponseError::ParseError(e.to_string()))?,
        );

        let api_key = self
            .viet_qr_config
            .api_key
            .as_deref()
            .ok_or_else(|| ResponseError::ParseError("missing VIET_QR_API_KEY".into()))?;
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(api_key).map_err(|e| ResponseError::ParseError(e.to_string()))?,
        );

        Ok(headers)
    }
    fn build_payload(&self) -> Value {
        serde_json::to_value(&self.transaction_info).expect("serialize failed")
    }
}

impl VietQRClient {
    fn new(
        account_no: &str,
        account_name: &str,
        acq_id: &str,
        add_info: &str,
        amount: &str,
    ) -> Self {
        let transaction_info =
            TransactionInformation::new(account_no, account_name, acq_id, add_info, amount);
        let viet_qr_config = VietQRConfig::init_from_env().expect("Env var not found");
        Self {
            viet_qr_config,
            transaction_info,
        }
    }
}

pub struct PaymentClient {
    http: Client,
    strategy: Arc<dyn ClientStrategy>,
}

impl PaymentClient {
    const VIET_QR_URL: &'static str = "https://api.vietqr.io/v2/generate";
    pub fn new(strategy: Arc<dyn ClientStrategy>) -> Self {
        Self {
            http: build_client(),
            strategy,
        }
    }
    pub fn set_bank_account(
        account_no: &str,
        account_name: &str,
        acq_id: &str,
        add_info: &str,
        amount: &str,
    ) -> Self {
        Self::new(Arc::new(VietQRClient::new(
            account_no,
            account_name,
            acq_id,
            add_info,
            amount,
        )))
    }
    pub async fn get_qr_code(&self) -> Result<VietQRResponse> {
        let headers = self.strategy.build_headers()?;
        let payload = self.strategy.build_payload();
        let response = self
            .http
            .post(Self::VIET_QR_URL)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        response.error_for_status_ref()?;
        let res = response.json::<VietQRResponse>().await?;
        Ok(res)
    }
}

fn build_client() -> Client {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to build client");
    client
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VietQRResponse {
    pub code: String,
    pub desc: String,
    pub data: VietQRData,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VietQRData {
    pub qr_code: String,
    #[serde(rename = "qrDataURL")]
    pub qr_data_url: String,
}

#[derive(Error, Debug)]
pub enum ResponseError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),
}

pub type Result<T> = std::result::Result<T, ResponseError>;
