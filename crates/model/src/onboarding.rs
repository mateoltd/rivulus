//! Onboarding + welcome.
use serde::{Deserialize, Serialize};
/// Onboarding prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingPrompt {
    /// Prompt id.
    pub id: Box<str>,
    /// Title.
    #[serde(default)]
    pub title: Option<Box<str>>,
}
