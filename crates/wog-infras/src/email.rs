use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};

use crate::DefaultConfig;

#[derive(thiserror::Error, Debug)]
pub enum EmailError {
    #[error("Failed to build email message: {0}")]
    Build(String),
    #[error("Failed to send email: {0}")]
    Send(String),
}

/// Sends transactional emails over SMTP.
///
/// If `SMTP_HOST` isn't configured, sends are logged instead of attempted,
/// so a server running without SMTP credentials still starts and works —
/// only the emails themselves don't go out.
#[derive(Clone)]
pub struct EmailService {
    transport: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from_email: String,
    from_name: String,
}

impl EmailService {
    pub fn new(config: &DefaultConfig) -> Self {
        let transport = if config.smtp_host.trim().is_empty() {
            tracing::warn!("SMTP_HOST is not set; verification emails will only be logged");
            None
        } else {
            match AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host) {
                Ok(builder) => Some(
                    builder
                        .port(config.smtp_port)
                        .credentials(Credentials::new(
                            config.smtp_username.clone(),
                            config.smtp_password.clone(),
                        ))
                        .build(),
                ),
                Err(e) => {
                    tracing::error!("Invalid SMTP_HOST '{}': {}", config.smtp_host, e);
                    None
                }
            }
        };

        Self {
            transport,
            from_email: config.smtp_from_email.clone(),
            from_name: config.smtp_from_name.clone(),
        }
    }

    pub async fn send_verification_email(
        &self,
        to_email: &str,
        to_username: &str,
        verify_url: &str,
    ) -> Result<(), EmailError> {
        let Some(transport) = &self.transport else {
            tracing::info!(
                email = %to_email,
                url = %verify_url,
                "SMTP not configured; skipping verification email send"
            );
            return Ok(());
        };

        // Build mailboxes from the validated email address plus a plain display
        // name, rather than parsing a hand-formatted "Name <addr>" string --
        // the latter chokes on ordinary display names (spaces, punctuation,
        // non-ASCII, ...) since it has to satisfy RFC 5322's unquoted-phrase
        // grammar. `Mailbox::new` takes care of encoding the name correctly.
        let from_address: Address = self.from_email.parse().map_err(|e| {
            EmailError::Build(format!("Invalid from address '{}': {e}", self.from_email))
        })?;
        let from = Mailbox::new(Some(self.from_name.clone()), from_address);

        let to_address: Address = to_email
            .parse()
            .map_err(|e| EmailError::Build(format!("Invalid to address '{to_email}': {e}")))?;
        let to = Mailbox::new(Some(to_username.to_string()), to_address);

        let text_body = format!(
            "Hi {to_username},\n\n\
             Please verify your email address by opening the link below:\n\n\
             {verify_url}\n\n\
             This link expires in 24 hours. If you didn't create this account, you can ignore this email.\n"
        );
        let html_body = format!(
            "<p>Hi {to_username},</p>\
             <p>Please verify your email address by clicking the link below:</p>\
             <p><a href=\"{verify_url}\">Verify my email</a></p>\
             <p>This link expires in 24 hours. If you didn't create this account, you can ignore this email.</p>"
        );

        let email = Message::builder()
            .from(from)
            .to(to)
            .subject("Verify your email")
            .multipart(MultiPart::alternative_plain_html(text_body, html_body))
            .map_err(|e| EmailError::Build(e.to_string()))?;

        transport
            .send(email)
            .await
            .map(|_| ())
            .map_err(|e| EmailError::Send(e.to_string()))
    }
}
