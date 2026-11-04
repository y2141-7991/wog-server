use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use reqwest::{Client, Url};
use serde_json::Value;

use crate::{BankDetails, PaymentError, PaymentProvider, Result, Transfer};

/// SePay's transaction timestamps are Vietnam time (UTC+7).
const VIETNAM_OFFSET_SECS: i32 = 7 * 3600;

#[derive(Debug, Clone)]
pub struct SepayConfig {
    /// Transactions API root, e.g. `https://my.sepay.vn/userapi`.
    pub api_base: String,
    /// Bearer token for the transactions API; without it only the QR code
    /// and bank details work.
    pub api_token: Option<String>,
    /// QR image endpoint, e.g. `https://vietqr.app/img`.
    pub qr_base_url: String,
    pub bank: BankDetails,
}

pub struct SepayClient {
    http: Client,
    config: SepayConfig,
}

impl SepayClient {
    pub fn new(config: SepayConfig) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build the HTTP client");
        Self { http, config }
    }
}

#[async_trait]
impl PaymentProvider for SepayClient {
    fn bank_details(&self) -> &BankDetails {
        &self.config.bank
    }

    fn qr_url(&self, amount_vnd: i64, transfer_content: &str) -> String {
        let amount = amount_vnd.to_string();
        Url::parse_with_params(
            &self.config.qr_base_url,
            [
                ("acc", self.config.bank.account_number.as_str()),
                ("bank", self.config.bank.bank_name.as_str()),
                ("amount", amount.as_str()),
                ("des", transfer_content),
                ("template", "compact"),
            ],
        )
        .map(String::from)
        .unwrap_or_else(|_| self.config.qr_base_url.clone())
    }

    fn can_list_transfers(&self) -> bool {
        self.config.api_token.is_some()
    }

    async fn recent_transfers(&self, since: DateTime<Utc>) -> Result<Vec<Transfer>> {
        let token = self
            .config
            .api_token
            .as_deref()
            .ok_or(PaymentError::NotConfigured)?;
        let since_vn = since
            .with_timezone(&FixedOffset::east_opt(VIETNAM_OFFSET_SECS).expect("valid offset"))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let response = self
            .http
            .get(format!("{}/transactions/list", self.config.api_base))
            .bearer_auth(token)
            .query(&[
                ("account_number", self.config.bank.account_number.as_str()),
                ("transaction_date_min", since_vn.as_str()),
                ("limit", "200"),
            ])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(PaymentError::Status {
                status: status.as_u16(),
                body: body.chars().take(200).collect(),
            });
        }
        parse_transactions(&response.json::<Value>().await?)
    }
}

/// Map the transactions API's `transactions` array to transfers. Money
/// fields are read leniently: SePay sends amounts as numbers or as decimal
/// strings such as `"2277000.00"`. Rows with no incoming amount are skipped.
pub fn parse_transactions(body: &Value) -> Result<Vec<Transfer>> {
    let rows = body
        .get("transactions")
        .and_then(Value::as_array)
        .ok_or_else(|| PaymentError::Parse("missing `transactions` array".into()))?;

    let mut transfers = Vec::new();
    for row in rows {
        let amount = row.get("amount_in").and_then(lenient_i64).unwrap_or(0);
        if amount <= 0 {
            continue;
        }
        let id = row
            .get("id")
            .and_then(lenient_i64)
            .ok_or_else(|| PaymentError::Parse("transaction without a numeric id".into()))?;
        transfers.push(Transfer {
            id,
            incoming: true,
            amount_vnd: amount,
            content: row
                .get("transaction_content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            code: row
                .get("code")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_string),
            raw: row.clone(),
        });
    }
    Ok(transfers)
}

fn lenient_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)),
        Value::String(s) => s.trim().parse::<f64>().ok().map(|f| f.round() as i64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn client() -> SepayClient {
        SepayClient::new(SepayConfig {
            api_base: "https://my.sepay.vn/userapi".into(),
            api_token: None,
            qr_base_url: "https://vietqr.app/img".into(),
            bank: BankDetails {
                bank_name: "Vietcombank".into(),
                account_number: "0123456789".into(),
                account_holder: "WOG JSC".into(),
            },
        })
    }

    #[test]
    fn qr_url_carries_account_bank_amount_and_content() {
        let url = client().qr_url(299000, "WOG7K2M9QXA");
        assert_eq!(
            url,
            "https://vietqr.app/img?acc=0123456789&bank=Vietcombank&amount=299000&des=WOG7K2M9QXA&template=compact"
        );
    }

    #[test]
    fn transactions_are_parsed_leniently_and_outgoing_rows_skipped() {
        let body = json!({"status": 200, "transactions": [
            {"id": "92704", "amount_in": "299000.00", "amount_out": "0.00",
             "transaction_content": "WOG7K2M9QXA chuyen tien", "code": "WOG7K2M9QXA"},
            {"id": 92705, "amount_in": 0, "amount_out": 50000, "transaction_content": "fee"},
            {"id": 92706, "amount_in": 10000, "transaction_content": "hello", "code": ""}
        ]});
        let transfers = parse_transactions(&body).unwrap();
        assert_eq!(transfers.len(), 2);
        assert_eq!(transfers[0].id, 92704);
        assert_eq!(transfers[0].amount_vnd, 299000);
        assert_eq!(transfers[0].code.as_deref(), Some("WOG7K2M9QXA"));
        assert_eq!(transfers[1].id, 92706);
        assert_eq!(transfers[1].code, None);
    }

    #[test]
    fn a_body_without_transactions_is_an_error() {
        assert!(parse_transactions(&json!({"status": 500})).is_err());
    }
}
