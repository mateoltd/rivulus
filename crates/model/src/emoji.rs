//! Emojis + stickers.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Emoji.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emoji {
    /// Emoji id (none for unicode).
    #[serde(default)]
    pub id: Option<Id<crate::EmojiMarker>>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
    /// Animated.
    #[serde(default)]
    pub animated: bool,
}
/// Sticker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sticker {
    /// Sticker id.
    pub id: Id<crate::StickerMarker>,
    /// Name.
    pub name: Box<str>,
    /// Format type.
    #[serde(default)]
    pub format_type: u8,
}
