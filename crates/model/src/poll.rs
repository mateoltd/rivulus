//! Polls.
use serde::{Deserialize, Serialize};
/// Poll answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollAnswer {
    /// Answer id.
    pub answer_id: u64,
    /// Text.
    #[serde(default)]
    pub text: Option<Box<str>>,
}
/// Poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poll {
    /// Question text.
    #[serde(default)]
    pub question: Option<Box<str>>,
    /// Answers.
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
}
