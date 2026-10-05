//! Owned `ExecuteWebhook` builder (sync, no IO).
//!
//! Body carries `content`; URL query carries
//! `?wait&thread_id&with_counts&thread_name`.

/// Owned webhook-execute builder.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct ExecuteWebhook {
    content: Option<String>,
    thread_name: Option<String>,
    wait: bool,
    thread_id: Option<u64>,
    with_counts: bool,
}

impl ExecuteWebhook {
    /// New empty builder.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }
    /// Set `thread_name` (forum threads; sent as query param).
    pub fn thread_name(mut self, name: impl Into<String>) -> Self {
        self.thread_name = Some(name.into());
        self
    }
    /// Set `wait` (return the message object).
    pub fn wait(mut self, wait: bool) -> Self {
        self.wait = wait;
        self
    }
    /// Set `thread_id`.
    pub fn thread_id(mut self, id: u64) -> Self {
        self.thread_id = Some(id);
        self
    }
    /// Set `with_counts`.
    pub fn with_counts(mut self, with_counts: bool) -> Self {
        self.with_counts = with_counts;
        self
    }
    /// Validate without serializing.
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] on any limit violation.
    pub fn validate(&self) -> Result<(), common::Error> {
        if let Some(c) = &self.content {
            common::validate::content(c)?;
        }
        if let Some(t) = &self.thread_name {
            if t.is_empty() || t.chars().count() > 100 {
                return Err(common::Error::Validation(Box::from(
                    "thread_name must be 1-100 chars",
                )));
            }
        }
        Ok(())
    }
    /// Query string (`""` when no params), e.g. `?wait=true&thread_id=1`.
    #[must_use]
    pub fn query(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.wait {
            parts.push(String::from("wait=true"));
        }
        if let Some(id) = self.thread_id {
            let mut buf = itoa::Buffer::new();
            parts.push(format!("thread_id={}", buf.format(id)));
        }
        if self.with_counts {
            parts.push(String::from("with_counts=true"));
        }
        if let Some(t) = &self.thread_name {
            parts.push(format!("thread_name={}", encode_query(t)));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("?{}", parts.join("&"))
        }
    }
    /// Build JSON body bytes (validates first).
    ///
    /// # Errors
    /// Returns [`common::Error`] on validation or serialization failure.
    pub fn build(&self) -> Result<Vec<u8>, common::Error> {
        self.to_json_bytes()
    }
    /// Build JSON body bytes (validates first).
    ///
    /// # Errors
    /// Returns [`common::Error`] on validation or serialization failure.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, common::Error> {
        self.validate()?;
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            content: Option<&'a str>,
        }
        let payload = Payload {
            content: self.content.as_deref(),
        };
        common::json::to_vec(&payload).map_err(|e| common::Error::Deserialize {
            event: Box::from("ExecuteWebhook"),
            reason: common::error::truncate_source(&e.to_string()),
        })
    }
}

/// Minimal query percent-encoding (unreserved `A-Z a-z 0-9 - _ . ~` pass).
fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_builder() {
        let b = ExecuteWebhook::new()
            .content("hi")
            .wait(true)
            .thread_id(7)
            .with_counts(true)
            .thread_name("a b");
        assert!(b.validate().is_ok());
        assert_eq!(
            b.query(),
            "?wait=true&thread_id=7&with_counts=true&thread_name=a%20b"
        );
        assert!(!b.build().unwrap_or_default().is_empty());
        assert_eq!(ExecuteWebhook::new().query(), "");
    }

    #[test]
    fn content_limit() {
        let big = "x".repeat(2001);
        assert!(ExecuteWebhook::new().content(big).validate().is_err());
    }
}
