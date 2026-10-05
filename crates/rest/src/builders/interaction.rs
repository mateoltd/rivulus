//! Owned interaction-callback builder (sync, no IO).
//!
//! Validation splits on `is_components_v2`: V1 `<= 5` rows x `5` per row,
//! V2 `<= 40` top-level (via `model::components_v2`).

/// Owned interaction callback payload builder.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct InteractionCallback {
    kind: u8,
    content: Option<String>,
    components: Vec<model::Component>,
    is_components_v2: bool,
    ephemeral: bool,
}

impl InteractionCallback {
    /// New builder with callback `kind` (default `4` = channel message).
    pub fn new(kind: u8) -> Self {
        Self {
            kind,
            content: None,
            components: Vec::new(),
            is_components_v2: false,
            ephemeral: false,
        }
    }
    /// Set content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }
    /// Push one top-level component.
    pub fn component(mut self, component: model::Component) -> Self {
        self.components.push(component);
        self
    }
    /// Set the components-V2 flag.
    pub fn components_v2(mut self, v2: bool) -> Self {
        self.is_components_v2 = v2;
        self
    }
    /// Set ephemeral (`flags: 64`).
    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }
    /// Validate without serializing.
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] on unknown callback kinds or
    /// any limit violation.
    pub fn validate(&self) -> Result<(), common::Error> {
        match self.kind {
            1 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 12 => {}
            _ => {
                return Err(common::Error::Validation(Box::from(
                    "unknown callback type",
                )));
            }
        }
        if let Some(c) = &self.content {
            common::validate::content(c)?;
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
        let mut flags: u64 = 0;
        if self.ephemeral {
            flags |= 64;
        }
        if self.is_components_v2 {
            flags |= 1 << 15;
        }
        #[derive(serde::Serialize)]
        struct Data<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            content: Option<&'a str>,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            components: &'a [model::Component],
            #[serde(skip_serializing_if = "Option::is_none")]
            flags: Option<u64>,
        }
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "type")]
            kind: u8,
            #[serde(skip_serializing_if = "Option::is_none")]
            data: Option<Data<'a>>,
        }
        let needs_data = self.content.is_some() || !self.components.is_empty() || flags != 0;
        let payload = Payload {
            kind: self.kind,
            data: needs_data.then(|| Data {
                content: self.content.as_deref(),
                components: &self.components,
                flags: (flags != 0).then_some(flags),
            }),
        };
        common::json::to_vec(&payload).map_err(|e| common::Error::Deserialize {
            event: Box::from("InteractionCallback"),
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
    fn split_validation() {
        let mut v1 = InteractionCallback::new(4).content("hi");
        for _ in 0..6 {
            v1 = v1.component(row());
        }
        assert!(v1.validate().is_err());
        let mut v2 = InteractionCallback::new(4)
            .content("hi")
            .components_v2(true);
        for _ in 0..6 {
            v2 = v2.component(row());
        }
        assert!(v2.validate().is_ok());
        assert!(!v2.build().unwrap_or_default().is_empty());
        assert!(InteractionCallback::new(3).validate().is_err());
    }
}
