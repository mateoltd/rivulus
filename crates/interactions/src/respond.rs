//! Interaction response builders (ACK within 3s or defer; token valid 15min).
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Interaction callback type numbers (no 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ResponseKind {
    /// Pong (answers Ping) = 1.
    Pong = 1,
    /// Channel message with source = 4.
    ChannelMessage = 4,
    /// Deferred channel message = 5.
    DeferredChannel = 5,
    /// Deferred update = 6.
    DeferredUpdate = 6,
    /// Update message = 7.
    UpdateMessage = 7,
    /// Autocomplete result (`<= 25` choices) = 8.
    AutocompleteResult = 8,
    /// Modal = 9.
    Modal = 9,
    /// Premium required = 10.
    PremiumRequired = 10,
    /// Launch activity = 12.
    LaunchActivity = 12,
}

impl ResponseKind {
    /// Wire number.
    #[must_use]
    pub fn number(self) -> u8 {
        self as u8
    }

    /// Parse a wire number (unknown, incl. `11`, => [`common::Error::Validation`]).
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] for unknown callback numbers.
    pub fn from_number(n: u8) -> Result<Self, common::Error> {
        match n {
            1 => Ok(Self::Pong),
            4 => Ok(Self::ChannelMessage),
            5 => Ok(Self::DeferredChannel),
            6 => Ok(Self::DeferredUpdate),
            7 => Ok(Self::UpdateMessage),
            8 => Ok(Self::AutocompleteResult),
            9 => Ok(Self::Modal),
            10 => Ok(Self::PremiumRequired),
            12 => Ok(Self::LaunchActivity),
            _ => Err(common::Error::Validation(Box::from(
                "unknown callback type",
            ))),
        }
    }
}

impl serde::Serialize for ResponseKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.number())
    }
}

impl<'de> serde::Deserialize<'de> for ResponseKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = u8::deserialize(d)?;
        Self::from_number(n).map_err(serde::de::Error::custom)
    }
}

/// One autocomplete choice (`name` shown, `value` returned).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutocompleteChoice {
    /// Display name.
    pub name: Box<str>,
    /// Option value.
    pub value: Box<str>,
}

/// Owned interaction response.
///
/// Serializes to `{ "type": <callback number>, "data": {...} }` with
/// `flags: 64` when `ephemeral`. `AutocompleteResult` responses carry
/// `choices` (enforced `<= 25` by [`Response::validate`]).
#[derive(Debug, Clone)]
pub struct Response {
    /// Callback kind.
    pub kind: ResponseKind,
    /// Message content, if any.
    pub content: Option<Box<str>>,
    /// Ephemeral (`flags: 64`).
    pub ephemeral: bool,
    /// Autocomplete choices (only for [`ResponseKind::AutocompleteResult`]).
    pub choices: Vec<AutocompleteChoice>,
}

impl Response {
    /// New response of `kind`.
    #[must_use]
    pub fn new(kind: ResponseKind) -> Self {
        Self {
            kind,
            content: None,
            ephemeral: false,
            choices: Vec::new(),
        }
    }

    /// Set content.
    #[must_use]
    pub fn content(mut self, content: impl Into<Box<str>>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Set ephemeral (`flags: 64`).
    #[must_use]
    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }

    /// Set autocomplete choices.
    #[must_use]
    pub fn choices(mut self, choices: Vec<AutocompleteChoice>) -> Self {
        self.choices = choices;
        self
    }

    /// Validate (content limits + `<= 25` autocomplete choices).
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] on over-long content or more
    /// than [`common::validate::AUTOCOMPLETE_MAX`] choices.
    pub fn validate(&self) -> Result<(), common::Error> {
        if let Some(c) = &self.content {
            common::validate::content(c)?;
        }
        if self.kind == ResponseKind::AutocompleteResult
            && self.choices.len() > common::validate::AUTOCOMPLETE_MAX
        {
            return Err(common::Error::Validation(Box::from(
                "autocomplete must have <= 25 choices",
            )));
        }
        Ok(())
    }

    /// Build callback JSON bytes (validates first).
    ///
    /// # Errors
    /// Returns [`common::Error`] on validation or serialization failure.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, common::Error> {
        self.validate()?;
        let mut flags: u64 = 0;
        if self.ephemeral {
            flags |= 64;
        }
        #[derive(Serialize)]
        struct Data<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            content: Option<&'a str>,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            choices: &'a [AutocompleteChoice],
            #[serde(skip_serializing_if = "Option::is_none")]
            flags: Option<u64>,
        }
        #[derive(Serialize)]
        struct Payload<'a> {
            #[serde(rename = "type")]
            kind: u8,
            #[serde(skip_serializing_if = "Option::is_none")]
            data: Option<Data<'a>>,
        }
        let needs_data = self.content.is_some() || !self.choices.is_empty() || flags != 0;
        let payload = Payload {
            kind: self.kind.number(),
            data: needs_data.then(|| Data {
                content: self.content.as_deref(),
                choices: &self.choices,
                flags: (flags != 0).then_some(flags),
            }),
        };
        common::json::to_vec(&payload).map_err(|e| common::Error::Deserialize {
            event: Box::from("Response"),
            reason: common::error::truncate_source(&e.to_string()),
        })
    }
}

/// True when `elapsed` is within the `<= 3s` ACK deadline (else defer first).
#[must_use]
pub fn ack_deadline_ok(elapsed: Duration) -> bool {
    elapsed <= Duration::from_secs(3)
}

/// Followup / edit / delete URL for an interaction token.
///
/// `None` message id => `webhooks/{app_id}/{token}` (create followup);
/// `Some` => `webhooks/{app_id}/{token}/messages/{id}` (edit/delete).
/// `token` is a [`secrecy::SecretString`]: it is exposed only into the
/// returned URL (which callers must treat as secret) and is never logged.
///
/// Interaction tokens expire after 15 minutes.
#[must_use]
pub fn followup_url(app_id: u64, token: &secrecy::SecretString, message_id: Option<u64>) -> String {
    use secrecy::ExposeSecret;
    let exposed: &str = token.expose_secret();
    match message_id {
        Some(id) => format!(
            "{}/webhooks/{}/{}/messages/{}",
            rest::BASE,
            app_id,
            exposed,
            id
        ),
        None => format!("{}/webhooks/{}/{}", rest::BASE, app_id, exposed),
    }
}

/// Callback response payload.
#[derive(Debug, Clone, Serialize)]
pub struct CallbackData {
    /// Callback type number.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Data (content, embeds, components, flags).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<CallbackBody>,
}

/// Callback body.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CallbackBody {
    /// Content (validated <= 2000 by caller).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Box<str>>,
    /// Ephemeral flag (64) etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Build a channel-message response (callback 4).
///
/// # Errors
/// Returns [`common::Error::Validation`] when content is too long.
pub fn respond(content: &str, ephemeral: bool) -> Result<CallbackData, common::Error> {
    common::validate::content(content)?;
    Ok(CallbackData {
        kind: 4,
        data: Some(CallbackBody {
            content: Some(Box::from(content)),
            flags: ephemeral.then_some(64),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(i: usize) -> AutocompleteChoice {
        AutocompleteChoice {
            name: Box::from(format!("c{i}")),
            value: Box::from(format!("v{i}")),
        }
    }

    #[test]
    fn kind_numbers() {
        assert_eq!(ResponseKind::Pong.number(), 1);
        assert_eq!(ResponseKind::ChannelMessage.number(), 4);
        assert_eq!(ResponseKind::DeferredChannel.number(), 5);
        assert_eq!(ResponseKind::DeferredUpdate.number(), 6);
        assert_eq!(ResponseKind::UpdateMessage.number(), 7);
        assert_eq!(ResponseKind::AutocompleteResult.number(), 8);
        assert_eq!(ResponseKind::Modal.number(), 9);
        assert_eq!(ResponseKind::PremiumRequired.number(), 10);
        assert_eq!(ResponseKind::LaunchActivity.number(), 12);
        assert!(ResponseKind::from_number(11).is_err());
        assert!(ResponseKind::from_number(3).is_err());
        assert_eq!(
            ResponseKind::from_number(8).unwrap_or(ResponseKind::Pong),
            ResponseKind::AutocompleteResult
        );
    }

    #[test]
    fn autocomplete_cap() {
        let ok: Vec<AutocompleteChoice> = (0..25).map(choice).collect();
        let over: Vec<AutocompleteChoice> = (0..26).map(choice).collect();
        assert!(Response::new(ResponseKind::AutocompleteResult)
            .choices(ok)
            .validate()
            .is_ok());
        assert!(Response::new(ResponseKind::AutocompleteResult)
            .choices(over)
            .validate()
            .is_err());
        let bytes = Response::new(ResponseKind::AutocompleteResult)
            .choices((0..2).map(choice).collect())
            .to_json_bytes()
            .unwrap_or_default();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn ephemeral_flag_and_content() {
        let bytes = Response::new(ResponseKind::ChannelMessage)
            .content("hi")
            .ephemeral(true)
            .to_json_bytes()
            .unwrap_or_default();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        assert_eq!(v["type"], 4);
        assert_eq!(v["data"]["content"], "hi");
        assert_eq!(v["data"]["flags"], 64);
        // Over-long content rejected.
        assert!(Response::new(ResponseKind::ChannelMessage)
            .content("x".repeat(2001))
            .validate()
            .is_err());
    }

    #[test]
    fn ack_deadline() {
        assert!(ack_deadline_ok(Duration::from_secs(2)));
        assert!(ack_deadline_ok(Duration::from_secs(3)));
        assert!(!ack_deadline_ok(Duration::from_secs(4)));
        assert!(!ack_deadline_ok(Duration::from_millis(3001)));
    }

    #[test]
    fn followup_urls() {
        let token = secrecy::SecretString::from(String::from("tok-abc"));
        // Token Debug never leaks.
        assert!(!format!("{token:?}").contains("tok-abc"));
        let base = followup_url(123, &token, None);
        assert_eq!(base, format!("{}/webhooks/123/tok-abc", rest::BASE));
        let msg = followup_url(123, &token, Some(456));
        assert_eq!(
            msg,
            format!("{}/webhooks/123/tok-abc/messages/456", rest::BASE)
        );
    }

    #[test]
    fn webhook_token_never_debug_logged() {
        // Webhook tokens are secrets: `SecretString` Debug must redact, and
        // the followup URL (which embeds the token) must be treated as
        // secret - wrapping it back in `SecretString` must also redact.
        let raw = "webhook-token-secret-xyz";
        let token = secrecy::SecretString::from(String::from(raw));
        assert!(!format!("{token:?}").contains(raw));
        let url = followup_url(123, &token, None);
        assert!(url.contains(raw), "URL must embed the token for Discord");
        let wrapped = secrecy::SecretString::from(url);
        assert!(
            !format!("{wrapped:?}").contains(raw),
            "wrapped webhook URL Debug must redact the token"
        );
    }
}
