//! Partials for uncached dispatch references.
use crate::Id;
/// Partial entity (frozen set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PartialKind {
    /// User.
    User,
    /// Channel.
    Channel,
    /// Guild member.
    GuildMember,
    /// Message.
    Message,
    /// Reaction.
    Reaction,
    /// Scheduled event.
    GuildScheduledEvent,
    /// Thread member.
    ThreadMember,
}
/// Partial reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Partial {
    /// Kind.
    pub kind: PartialKind,
    /// Referenced id (guild/channel/message/user namespace as u64).
    pub id: u64,
}
impl Partial {
    /// Build a user partial.
    #[must_use]
    pub fn user(id: Id<crate::UserMarker>) -> Self {
        Self {
            kind: PartialKind::User,
            id: id.get(),
        }
    }
    /// Build a message partial.
    #[must_use]
    pub fn message(id: Id<crate::MessageMarker>) -> Self {
        Self {
            kind: PartialKind::Message,
            id: id.get(),
        }
    }
}
