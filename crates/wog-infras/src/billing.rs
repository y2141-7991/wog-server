//! Provider-independent billing rules: configuration, payment codes, and
//! webhook authentication. The database side lives in `repos::billing` and the
//! orchestration in `services::billing`.

use chrono::Duration;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::DefaultConfig;

pub use payment_client::Transfer;

/// Characters after the prefix in a payment code. Fixed so it can be matched
/// back out of a bank's transfer content; set the same length in SePay's
/// payment-code setting (suffix min = max = 8, letters and digits).
pub const CODE_SUFFIX_LEN: usize = 8;

/// 32 symbols without `0 O 1 I`, which people misread and banks may rewrite.
/// A power of two, so mapping random bytes onto it is unbiased.
const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// A plan of the same tier can be bought again only this close to expiry.
pub const RENEWAL_WINDOW_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct BillingConfig {
    pub webhook_api_key: String,
    /// Uppercase, 2-5 characters (SePay's limit).
    pub code_prefix: String,
    pub order_ttl: Duration,
}

impl BillingConfig {
    /// `None` (with a log line saying why) unless everything billing needs is
    /// configured, so a dev setup without SePay still boots.
    pub fn from_config(cfg: &DefaultConfig) -> Option<Self> {
        let missing: Vec<&str> = [
            ("SEPAY_WEBHOOK_API_KEY", &cfg.sepay_webhook_api_key),
            ("SEPAY_BANK_NAME", &cfg.sepay_bank_name),
            ("SEPAY_ACCOUNT_NUMBER", &cfg.sepay_account_number),
            ("SEPAY_ACCOUNT_HOLDER", &cfg.sepay_account_holder),
        ]
        .into_iter()
        .filter(|(_, value)| value.trim().is_empty())
        .map(|(name, _)| name)
        .collect();
        if !missing.is_empty() {
            tracing::warn!(
                "Billing is disabled: set {} to enable plan purchases",
                missing.join(", ")
            );
            return None;
        }

        let prefix = cfg.sepay_payment_code_prefix.trim().to_ascii_uppercase();
        if !(2..=5).contains(&prefix.len()) || !prefix.bytes().all(|b| b.is_ascii_alphanumeric()) {
            tracing::warn!(
                "Billing is disabled: SEPAY_PAYMENT_CODE_PREFIX must be 2-5 letters or digits"
            );
            return None;
        }
        if cfg.order_ttl_minutes < 1 {
            tracing::warn!("Billing is disabled: ORDER_TTL_MINUTES must be at least 1");
            return None;
        }
        Some(Self {
            webhook_api_key: cfg.sepay_webhook_api_key.trim().to_string(),
            code_prefix: prefix,
            order_ttl: Duration::minutes(cfg.order_ttl_minutes),
        })
    }
}

/// A fresh payment code such as `WOG7K2M9QXA`: the prefix plus 8 random
/// symbols from a CSPRNG-backed v4 UUID.
pub fn generate_payment_code(prefix: &str) -> String {
    let random = Uuid::new_v4();
    let mut code = String::with_capacity(prefix.len() + CODE_SUFFIX_LEN);
    code.push_str(prefix);
    for byte in random.as_bytes().iter().take(CODE_SUFFIX_LEN) {
        code.push(CODE_ALPHABET[(byte & 0x1f) as usize] as char);
    }
    code
}

/// Every payment-code candidate in a transfer, best first: the code the
/// provider extracted, then any `PREFIX + 8 characters` found in the content,
/// then the same after dropping everything but letters and digits (banks
/// sometimes insert spaces or punctuation). Candidates are only guesses; the
/// database decides which one is a real order.
pub fn extract_payment_codes(
    prefix: &str,
    provider_code: Option<&str>,
    content: &str,
) -> Vec<String> {
    let prefix = prefix.to_ascii_uppercase();
    let upper = content.to_ascii_uppercase();
    let squeezed: String = upper.chars().filter(char::is_ascii_alphanumeric).collect();

    let mut codes: Vec<String> = Vec::new();
    let mut push = |code: String| {
        if !code.is_empty() && !codes.contains(&code) {
            codes.push(code);
        }
    };
    if let Some(code) = provider_code {
        push(code.trim().to_ascii_uppercase());
    }
    for haystack in [&upper, &squeezed] {
        let mut from = 0;
        while let Some(found) = haystack[from..].find(&prefix) {
            let start = from + found;
            let suffix: String = haystack[start + prefix.len()..]
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .take(CODE_SUFFIX_LEN)
                .collect();
            if suffix.len() == CODE_SUFFIX_LEN {
                push(format!("{prefix}{suffix}"));
            }
            from = start + 1;
        }
    }
    codes
}

/// The webhook fields we rely on. Nothing else in the body is trusted.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SepayWebhook {
    id: i64,
    transfer_type: String,
    transfer_amount: i64,
    #[serde(default)]
    content: String,
    #[serde(default)]
    code: Option<String>,
}

/// Parse a SePay webhook body into a transfer.
pub fn parse_webhook(body: &[u8]) -> Result<Transfer, serde_json::Error> {
    let raw: serde_json::Value = serde_json::from_slice(body)?;
    let hook: SepayWebhook = serde_json::from_value(raw.clone())?;
    Ok(Transfer {
        id: hook.id,
        incoming: hook.transfer_type.eq_ignore_ascii_case("in"),
        amount_vnd: hook.transfer_amount,
        content: hook.content,
        code: hook.code.filter(|c| !c.trim().is_empty()),
        raw,
    })
}

/// Check SePay's `Authorization: Apikey <key>` header in constant time.
pub fn webhook_key_matches(header: Option<&str>, expected: &str) -> bool {
    let Some(header) = header else { return false };
    let Some((scheme, key)) = header.trim().split_once(' ') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("apikey") {
        return false;
    }
    // Comparing fixed-length digests keeps the comparison time independent of
    // where (or whether) the keys differ, and of their lengths.
    let given = Sha256::digest(key.trim().as_bytes());
    let wanted = Sha256::digest(expected.as_bytes());
    given
        .iter()
        .zip(wanted.iter())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_codes_have_the_prefix_and_an_unambiguous_suffix() {
        for _ in 0..500 {
            let code = generate_payment_code("WOG");
            assert_eq!(code.len(), 3 + CODE_SUFFIX_LEN);
            assert!(code.starts_with("WOG"));
            assert!(
                code[3..].bytes().all(|b| CODE_ALPHABET.contains(&b)),
                "{code}"
            );
        }
    }

    #[test]
    fn generated_codes_do_not_collide_in_practice() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..20_000 {
            assert!(seen.insert(generate_payment_code("WOG")));
        }
    }

    #[test]
    fn the_provider_code_wins_and_content_is_scanned_too() {
        let codes = extract_payment_codes(
            "WOG",
            Some("wog7k2m9qxa"),
            "nguyen van a WOG7K2M9QXA thanh toan",
        );
        assert_eq!(codes, vec!["WOG7K2M9QXA"]);
    }

    #[test]
    fn a_code_is_found_in_lowercase_content_without_a_provider_code() {
        assert_eq!(
            extract_payment_codes("WOG", None, "chuyen tien wog7k2m9qxa"),
            vec!["WOG7K2M9QXA"]
        );
    }

    #[test]
    fn a_code_broken_up_by_the_bank_is_still_found() {
        assert_eq!(
            extract_payment_codes("WOG", None, "WOG 7K2M-9QXA"),
            vec!["WOG7K2M9QXA"]
        );
    }

    #[test]
    fn a_longer_run_after_the_prefix_is_cut_to_the_code_length() {
        assert_eq!(
            extract_payment_codes("WOG", None, "WOG7K2M9QXA123"),
            vec!["WOG7K2M9QXA"]
        );
    }

    #[test]
    fn content_without_a_code_yields_nothing() {
        assert!(extract_payment_codes("WOG", None, "tien an trua").is_empty());
        assert!(extract_payment_codes("WOG", None, "WOG123").is_empty());
    }

    #[test]
    fn a_webhook_body_becomes_a_transfer() {
        let body =
            br#"{"id":92704,"gateway":"Vietcombank","transferType":"in","transferAmount":299000,
            "content":"WOG7K2M9QXA chuyen tien","code":null,"accountNumber":"1017588888"}"#;
        let t = parse_webhook(body).unwrap();
        assert_eq!((t.id, t.incoming, t.amount_vnd), (92704, true, 299000));
        assert_eq!(t.code, None);
        assert_eq!(t.raw["gateway"], "Vietcombank");

        let out = br#"{"id":1,"transferType":"out","transferAmount":5}"#;
        assert!(!parse_webhook(out).unwrap().incoming);
        assert!(parse_webhook(b"{}").is_err());
        assert!(parse_webhook(b"not json").is_err());
    }

    #[test]
    fn webhook_auth_requires_the_apikey_scheme_and_the_exact_key() {
        assert!(webhook_key_matches(Some("Apikey s3cret"), "s3cret"));
        assert!(webhook_key_matches(Some("apikey s3cret"), "s3cret"));
        assert!(!webhook_key_matches(Some("Apikey wrong"), "s3cret"));
        assert!(!webhook_key_matches(Some("Bearer s3cret"), "s3cret"));
        assert!(!webhook_key_matches(Some("s3cret"), "s3cret"));
        assert!(!webhook_key_matches(None, "s3cret"));
        assert!(!webhook_key_matches(Some("Apikey "), "s3cret"));
    }
}
