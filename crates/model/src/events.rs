//! Dispatch `Event` (~90 variants + `Unknown` capped at 8 KiB).
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Cap for unknown payloads.
pub const UNKNOWN_CAP: usize = 8192;

/// Gateway dispatch event (fully owned, `Send + Sync + 'static`).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Event {
    /// Ready.
    Ready(Arc<Ready>),
    /// Resumed (synthetic facade-level).
    Resumed,
    /// Guild create (may be unavailable first).
    GuildCreate(Arc<crate::Guild>),
    /// Guild update (old, new).
    GuildUpdate(Option<Arc<crate::Guild>>, Arc<crate::Guild>),
    /// Guild delete.
    GuildDelete {
        /// Guild id.
        guild_id: crate::GuildId,
        /// Unavailable (vs removed).
        unavailable: bool,
    },
    /// Guild unavailable.
    GuildUnavailable(crate::GuildId),
    /// Guild available.
    GuildAvailable(Arc<crate::Guild>),
    /// Member add.
    MemberAdd(Arc<crate::Member>),
    /// Member remove.
    MemberRemove {
        /// Guild.
        guild_id: crate::GuildId,
        /// User.
        user_id: crate::UserId,
    },
    /// Member update (old, new).
    MemberUpdate(Option<Arc<crate::Member>>, Arc<crate::Member>),
    /// Members chunk.
    MembersChunk(Arc<crate::GuildMembersChunk>),
    /// Channel create.
    ChannelCreate(Arc<crate::Channel>),
    /// Channel update (old, new).
    ChannelUpdate(Option<Arc<crate::Channel>>, Arc<crate::Channel>),
    /// Channel delete.
    ChannelDelete(Arc<crate::Channel>),
    /// Channel pins update.
    ChannelPinsUpdate {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Last pin.
        last_pin: Option<Box<str>>,
    },
    /// Thread create.
    ThreadCreate(Arc<crate::Thread>),
    /// Thread update (old, new).
    ThreadUpdate(Option<Arc<crate::Thread>>, Arc<crate::Thread>),
    /// Thread delete.
    ThreadDelete(Arc<crate::Thread>),
    /// Thread list sync.
    ThreadListSync {
        /// Guild.
        guild_id: crate::GuildId,
    },
    /// Thread member update.
    ThreadMemberUpdate(Arc<crate::ThreadMember>),
    /// Thread members update.
    ThreadMembersUpdate {
        /// Thread.
        thread_id: crate::ChannelId,
    },
    /// Role create.
    RoleCreate {
        /// Guild.
        guild_id: crate::GuildId,
        /// Role.
        role: Arc<crate::Role>,
    },
    /// Role update (old, new).
    RoleUpdate(Option<Arc<crate::Role>>, Arc<crate::Role>),
    /// Role delete.
    RoleDelete {
        /// Guild.
        guild_id: crate::GuildId,
        /// Role.
        role_id: crate::RoleId,
    },
    /// Message create.
    MessageCreate(Arc<crate::Message>),
    /// Message update (old, new).
    MessageUpdate(Option<Arc<crate::Message>>, Arc<crate::Message>),
    /// Message delete.
    MessageDelete {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Message.
        message_id: crate::MessageId,
    },
    /// Bulk delete.
    MessageDeleteBulk {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Ids.
        ids: Vec<crate::MessageId>,
    },
    /// Reaction add.
    ReactionAdd {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Message.
        message_id: crate::MessageId,
        /// User.
        user_id: crate::UserId,
        /// Emoji name.
        emoji: Box<str>,
    },
    /// Reaction remove.
    ReactionRemove {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Message.
        message_id: crate::MessageId,
        /// User.
        user_id: crate::UserId,
    },
    /// Reaction remove all.
    ReactionRemoveAll {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Message.
        message_id: crate::MessageId,
    },
    /// Reaction remove emoji.
    ReactionRemoveEmoji {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Message.
        message_id: crate::MessageId,
    },
    /// Presence update.
    PresenceUpdate(Arc<crate::Presence>),
    /// Typing start.
    TypingStart {
        /// Channel.
        channel_id: crate::ChannelId,
        /// User.
        user_id: crate::UserId,
    },
    /// Voice state update (forwarded only v1).
    VoiceStateUpdate(Arc<crate::VoiceState>),
    /// Voice server update (forwarded only v1).
    VoiceServerUpdate(Arc<crate::VoiceServerUpdate>),
    /// Interaction create.
    InteractionCreate(Arc<crate::Interaction>),
    /// Webhooks update.
    WebhooksUpdate {
        /// Guild.
        guild_id: crate::GuildId,
        /// Channel.
        channel_id: crate::ChannelId,
    },
    /// Invite create.
    InviteCreate(Arc<crate::Invite>),
    /// Invite delete.
    InviteDelete {
        /// Channel.
        channel_id: crate::ChannelId,
        /// Code.
        code: Box<str>,
    },
    /// Emoji update.
    EmojiUpdate(crate::GuildId),
    /// Sticker update.
    StickerUpdate(crate::GuildId),
    /// Scheduled event create/update/delete.
    ScheduledCreate(Arc<crate::ScheduledEvent>),
    /// Scheduled update.
    ScheduledUpdate(Arc<crate::ScheduledEvent>),
    /// Scheduled delete.
    ScheduledDelete(Arc<crate::ScheduledEvent>),
    /// Stage create/update/delete.
    StageCreate(Arc<crate::StageInstance>),
    /// Stage update.
    StageUpdate(Arc<crate::StageInstance>),
    /// Stage delete.
    StageDelete(Arc<crate::StageInstance>),
    /// Auto-mod rule create/update/delete.
    AutoModRuleCreate(Arc<crate::AutoModRule>),
    /// Auto-mod rule update.
    AutoModRuleUpdate(Arc<crate::AutoModRule>),
    /// Auto-mod rule delete.
    AutoModRuleDelete(Arc<crate::AutoModRule>),
    /// Auto-mod action.
    AutoModAction(Arc<crate::AutoModAction>),
    /// Audit entry.
    AuditEntry(Arc<crate::AuditEntry>),
    /// Entitlement create/update/delete.
    EntitlementCreate(Arc<crate::Entitlement>),
    /// Entitlement update.
    EntitlementUpdate(Arc<crate::Entitlement>),
    /// Entitlement delete.
    EntitlementDelete(Arc<crate::Entitlement>),
    /// Poll vote add/remove.
    PollVoteAdd {
        /// Message.
        message_id: crate::MessageId,
        /// Answer.
        answer_id: u64,
    },
    /// Poll vote remove.
    PollVoteRemove {
        /// Message.
        message_id: crate::MessageId,
        /// Answer.
        answer_id: u64,
    },
    /// User update.
    UserUpdate(Arc<crate::User>),
    /// Soundboard sounds.
    SoundboardSounds(crate::GuildId),
    /// Subscription create/update/delete.
    SubscriptionCreate(Arc<crate::Entitlement>),
    /// Subscription update.
    SubscriptionUpdate(Arc<crate::Entitlement>),
    /// Subscription delete.
    SubscriptionDelete(Arc<crate::Entitlement>),
    /// Client-level synthetic: rate limited.
    RateLimited {
        /// Route template.
        route_template: Box<str>,
        /// Status.
        status: u16,
        /// Retry after.
        retry_after: f64,
    },
    /// Client-level synthetic: raw frame.
    Raw {
        /// Op.
        op: u8,
        /// Type.
        t: Option<Box<str>>,
        /// Seq.
        s: Option<u64>,
        /// Payload (capped).
        payload: Box<str>,
    },
    /// Client-level synthetic: cache swept.
    CacheSwept {
        /// Resource.
        resource: Box<str>,
        /// Count.
        count: usize,
    },
    /// Unknown dispatch (payload capped at 8 KiB).
    Unknown {
        /// Kind.
        kind: Box<str>,
        /// Seq.
        seq: Option<u64>,
        /// Payload.
        payload: Box<str>,
    },
}

impl Event {
    /// Build unknown with cap.
    #[must_use]
    pub fn unknown(kind: &str, seq: Option<u64>, payload: &str) -> Self {
        let p = if payload.len() > UNKNOWN_CAP {
            &payload[..UNKNOWN_CAP]
        } else {
            payload
        };
        Self::Unknown {
            kind: Box::from(kind),
            seq,
            payload: Box::from(p),
        }
    }
    /// Event kind name.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ready(_) => "READY",
            Self::MessageCreate(_) => "MESSAGE_CREATE",
            Self::InteractionCreate(_) => "INTERACTION_CREATE",
            Self::Unknown { .. } => "UNKNOWN",
            _ => "OTHER",
        }
    }
}

/// Minimal Ready payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ready {
    /// API version.
    pub v: u8,
    /// Session id.
    pub session_id: Box<str>,
    /// Resume URL (ALL resumes use this).
    pub resume_gateway_url: Box<str>,
    /// Self user.
    pub user: crate::User,
}
