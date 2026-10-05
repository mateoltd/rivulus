//! Soundboard sounds.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Soundboard sound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundboardSound {
    /// Sound id.
    pub sound_id: Id<crate::EmojiMarker>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
}
