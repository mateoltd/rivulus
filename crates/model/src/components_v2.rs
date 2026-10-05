//! Components incl. V2 (numbers per plans/04D; int wire format).
//!
//! Discord sends component `type` and button `style` as JSON integers.
//! Both enums carry an `Unknown(u8)` round-trip arm (manual `Deserialize`,
//! mirroring [`crate::ChannelType`]) so future Discord numbers parse and
//! re-serialize byte-identically instead of failing.
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Component kind numbers (Discord `type` int).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ComponentKind {
    /// ActionRow = 1.
    ActionRow,
    /// Button = 2.
    Button,
    /// StringSelect = 3.
    StringSelect,
    /// TextInput = 4.
    TextInput,
    /// UserSelect = 5.
    UserSelect,
    /// RoleSelect = 6.
    RoleSelect,
    /// MentionableSelect = 7.
    MentionableSelect,
    /// ChannelSelect = 8.
    ChannelSelect,
    /// Section = 9.
    Section,
    /// TextDisplay = 10.
    TextDisplay,
    /// Thumbnail = 11.
    Thumbnail,
    /// MediaGallery = 12.
    MediaGallery,
    /// File = 13.
    File,
    /// Separator = 14.
    Separator,
    /// Container = 17.
    Container,
    /// Label = 18.
    Label,
    /// Unknown future kind (round-trips).
    Unknown(u8),
}

impl ComponentKind {
    fn n(self) -> u8 {
        match self {
            Self::ActionRow => 1,
            Self::Button => 2,
            Self::StringSelect => 3,
            Self::TextInput => 4,
            Self::UserSelect => 5,
            Self::RoleSelect => 6,
            Self::MentionableSelect => 7,
            Self::ChannelSelect => 8,
            Self::Section => 9,
            Self::TextDisplay => 10,
            Self::Thumbnail => 11,
            Self::MediaGallery => 12,
            Self::File => 13,
            Self::Separator => 14,
            Self::Container => 17,
            Self::Label => 18,
            Self::Unknown(n) => n,
        }
    }
    fn from_n(n: u8) -> Self {
        match n {
            1 => Self::ActionRow,
            2 => Self::Button,
            3 => Self::StringSelect,
            4 => Self::TextInput,
            5 => Self::UserSelect,
            6 => Self::RoleSelect,
            7 => Self::MentionableSelect,
            8 => Self::ChannelSelect,
            9 => Self::Section,
            10 => Self::TextDisplay,
            11 => Self::Thumbnail,
            12 => Self::MediaGallery,
            13 => Self::File,
            14 => Self::Separator,
            17 => Self::Container,
            18 => Self::Label,
            n => Self::Unknown(n),
        }
    }
}
struct ComponentKindVisitor;
impl Visitor<'_> for ComponentKindVisitor {
    type Value = ComponentKind;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("component kind u8")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<ComponentKind, E> {
        Ok(ComponentKind::from_n(v as u8))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<ComponentKind, E> {
        Ok(ComponentKind::from_n(v as u8))
    }
}
impl<'de> Deserialize<'de> for ComponentKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ComponentKindVisitor)
    }
}
impl Serialize for ComponentKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.n())
    }
}
/// Button style (Discord `style` int).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ButtonStyle {
    /// Primary = 1.
    Primary,
    /// Secondary = 2.
    Secondary,
    /// Success = 3.
    Success,
    /// Danger = 4.
    Danger,
    /// Link = 5.
    Link,
    /// Premium = 6.
    Premium,
    /// Unknown future style (round-trips).
    Unknown(u8),
}

impl ButtonStyle {
    fn n(self) -> u8 {
        match self {
            Self::Primary => 1,
            Self::Secondary => 2,
            Self::Success => 3,
            Self::Danger => 4,
            Self::Link => 5,
            Self::Premium => 6,
            Self::Unknown(n) => n,
        }
    }
    fn from_n(n: u8) -> Self {
        match n {
            1 => Self::Primary,
            2 => Self::Secondary,
            3 => Self::Success,
            4 => Self::Danger,
            5 => Self::Link,
            6 => Self::Premium,
            n => Self::Unknown(n),
        }
    }
}
struct ButtonStyleVisitor;
impl Visitor<'_> for ButtonStyleVisitor {
    type Value = ButtonStyle;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("button style u8")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<ButtonStyle, E> {
        Ok(ButtonStyle::from_n(v as u8))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<ButtonStyle, E> {
        Ok(ButtonStyle::from_n(v as u8))
    }
}
impl<'de> Deserialize<'de> for ButtonStyle {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ButtonStyleVisitor)
    }
}
impl Serialize for ButtonStyle {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.n())
    }
}
/// Generic component node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    /// Kind.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// Custom id.
    #[serde(default)]
    pub custom_id: Option<Box<str>>,
    /// Label.
    #[serde(default)]
    pub label: Option<Box<str>>,
    /// Style (buttons).
    #[serde(default)]
    pub style: Option<ButtonStyle>,
    /// Children (rows/containers/sections).
    #[serde(default)]
    pub components: Vec<Component>,
    /// Content (TextDisplay).
    #[serde(default)]
    pub content: Option<Box<str>>,
}
/// Validate V1 (<=5 rows x 5).
///
/// # Errors
/// Returns [`common::Error::Validation`] on violation.
pub fn validate_v1(rows: &[Component]) -> Result<(), common::Error> {
    if rows.len() > common::validate::ROWS_MAX {
        return Err(common::Error::Validation(Box::from("too many rows")));
    }
    for r in rows {
        if r.components.len() > common::validate::ROW_BUTTONS_MAX {
            return Err(common::Error::Validation(Box::from("too many per row")));
        }
    }
    Ok(())
}
/// Validate V2 (top <=40, container <=10, text <=4000, no V1/V2 mixing enforced by caller flag).
///
/// # Errors
/// Returns [`common::Error::Validation`] on violation.
pub fn validate_v2(top: &[Component]) -> Result<(), common::Error> {
    use common::validate::{V2_CONTAINER_MAX, V2_TEXT_MAX, V2_TOP_MAX};
    if top.len() > V2_TOP_MAX {
        return Err(common::Error::Validation(Box::from(
            "too many top components",
        )));
    }
    for c in top {
        if c.kind == ComponentKind::Container && c.components.len() > V2_CONTAINER_MAX {
            return Err(common::Error::Validation(Box::from("container too big")));
        }
        if c.kind == ComponentKind::TextDisplay {
            if let Some(t) = &c.content {
                if t.len() > V2_TEXT_MAX {
                    return Err(common::Error::Validation(Box::from("text too long")));
                }
            }
        }
    }
    Ok(())
}
