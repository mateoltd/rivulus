//! Owned `CreateMessage` builder (sync, no IO).
//!
//! Validates via [`common::validate`] (`content <= 2000`,
//! `embeds <= 10`, components V1 `<= 5x5` vs V2 `<= 40` top-level) and
//! serializes via [`common::json::to_vec`].

use super::embed::CreateEmbed;

/// Owned message builder.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct CreateMessage {
    content: Option<String>,
    embeds: Vec<CreateEmbed>,
    components: Vec<model::Component>,
    is_components_v2: bool,
}

impl CreateMessage {
    /// New empty builder.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }
    /// Push one embed.
    pub fn embed(mut self, embed: CreateEmbed) -> Self {
        self.embeds.push(embed);
        self
    }
    /// Push one component.
    pub fn component(mut self, component: model::Component) -> Self {
        self.components.push(component);
        self
    }
    /// Set the components-V2 flag (flag `32768` on the wire).
    pub fn components_v2(mut self, v2: bool) -> Self {
        self.is_components_v2 = v2;
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
        if self.embeds.len() > common::validate::EMBEDS_MAX {
            return Err(common::Error::Validation(Box::from("embeds > 10")));
        }
        for e in &self.embeds {
            e.validate()?;
        }
        if self.is_components_v2 {
            model::components_v2::validate_v2(&self.components)?;
        } else {
            model::components_v2::validate_v1(&self.components)?;
        }
        Ok(())
    }
    /// Build JSON bytes (validates first).
    ///
    /// # Errors
    /// Returns [`common::Error`] on validation or serialization failure.
    pub fn build(&self) -> Result<Vec<u8>, common::Error> {
        self.to_json_bytes()
    }
    /// Build JSON bytes (validates first).
    ///
    /// # Errors
    /// Returns [`common::Error`] on validation or serialization failure.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, common::Error> {
        self.validate()?;
        let mut embeds: Vec<model::Embed> = Vec::with_capacity(self.embeds.len());
        for e in &self.embeds {
            embeds.push(e.build()?);
        }
        let flags: Option<u64> = if self.is_components_v2 {
            Some(1 << 15)
        } else {
            None
        };
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            content: Option<&'a str>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            embeds: Vec<model::Embed>,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            components: &'a [model::Component],
            #[serde(skip_serializing_if = "Option::is_none")]
            flags: Option<u64>,
        }
        let payload = Payload {
            content: self.content.as_deref(),
            embeds,
            components: &self.components,
            flags,
        };
        common::json::to_vec(&payload).map_err(|e| common::Error::Deserialize {
            event: Box::from("CreateMessage"),
            reason: common::error::truncate_source(&e.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> model::Component {
        model::Component {
            kind: model::ComponentKind::ActionRow,
            custom_id: None,
            label: None,
            style: None,
            components: Vec::new(),
            content: None,
        }
    }

    #[test]
    fn content_limit() {
        let big = "x".repeat(2001);
        let b = CreateMessage::new().content(big);
        assert!(b.validate().is_err());
        assert!(b.build().is_err());
    }

    #[test]
    fn embeds_limit() {
        let mut b = CreateMessage::new().content("hi");
        for _ in 0..11 {
            b = b.embed(CreateEmbed::new().description("e"));
        }
        assert!(b.validate().is_err());
    }

    #[test]
    fn v1_vs_v2_split() {
        let mut v1 = CreateMessage::new().content("hi");
        for _ in 0..6 {
            v1 = v1.component(row());
        }
        assert!(v1.validate().is_err());
        let mut v2 = CreateMessage::new().content("hi").components_v2(true);
        for _ in 0..6 {
            v2 = v2.component(row());
        }
        assert!(v2.validate().is_ok());
        let bytes = v2.to_json_bytes().unwrap_or_default();
        assert!(!bytes.is_empty());
    }
}
