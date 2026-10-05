//! Owned `CreateEmbed` builder (sync, no IO).
//!
//! Total of `title` + `description` chars must stay within
//! [`common::validate::EMBED_TOTAL_MAX`] (6000).

/// Owned embed builder.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct CreateEmbed {
    title: Option<String>,
    description: Option<String>,
    color: Option<u32>,
}

impl CreateEmbed {
    /// New empty builder.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    /// Set description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
    /// Set color.
    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }
    /// Validate (total chars `<= 6000`).
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] when over the limit.
    pub fn validate(&self) -> Result<(), common::Error> {
        let mut total: usize = 0;
        if let Some(t) = &self.title {
            total += t.chars().count();
        }
        if let Some(d) = &self.description {
            total += d.chars().count();
        }
        if total > common::validate::EMBED_TOTAL_MAX {
            return Err(common::Error::Validation(Box::from(
                "embed total chars > 6000",
            )));
        }
        Ok(())
    }
    /// Build into [`model::Embed`].
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] when over the limit.
    pub fn build(&self) -> Result<model::Embed, common::Error> {
        self.validate()?;
        Ok(model::Embed {
            title: self.title.as_deref().map(Box::from),
            description: self.description.as_deref().map(Box::from),
            color: self.color,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_limit() {
        let ok = CreateEmbed::new().title("hi").description("there").build();
        assert!(ok.is_ok());
        let big = "x".repeat(6001);
        let err = CreateEmbed::new().description(big).validate();
        assert!(matches!(err, Err(common::Error::Validation(_))));
    }
}
