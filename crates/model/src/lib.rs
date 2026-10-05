//! `model` - pure Discord types (serde only, no IO, no error type).
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod audit;
pub mod automod;
pub mod channel;
pub mod components_v2;
pub mod emoji;
pub mod entitlement;
pub mod events;
pub mod gateway_bot;
pub mod guild;
pub mod id;
pub mod intents;
pub mod interactions;
pub mod invite;
pub mod member;
pub mod message;
pub mod oauth;
pub mod onboarding;
pub mod partials;
pub mod permissions;
pub mod poll;
pub mod presence;
pub mod role;
pub mod scheduled;
pub mod soundboard;
pub mod stage;
pub mod thread;
pub mod user;
pub mod webhook;

pub use audit::AuditEntry;
pub use automod::{AutoModAction, AutoModRule};
pub use channel::{Channel, ChannelType};
pub use components_v2::{ButtonStyle, Component, ComponentKind};
pub use emoji::{Emoji, Sticker};
pub use entitlement::{Entitlement, Sku};
pub use events::{Event, Ready};
pub use gateway_bot::{GetGatewayBotResponse, SessionStartLimit};
pub use guild::{Guild, GuildMembersChunk, UnavailableGuild};
pub use id::{
    ApplicationId, ApplicationMarker, AuditId, AuditMarker, AutoModId, AutoModMarker, ChannelId,
    ChannelMarker, EmojiId, EmojiMarker, EntitlementId, EntitlementMarker, GuildId, GuildMarker,
    Id, InteractionId, InteractionMarker, MessageId, MessageMarker, Resource, RoleId, RoleMarker,
    ScheduledId, ScheduledMarker, SkuId, SkuMarker, StageId, StageMarker, StickerId, StickerMarker,
    ThreadId, ThreadMarker, UserId, UserMarker, WebhookId, WebhookMarker,
};
pub use intents::Intents;
pub use interactions::{CallbackType, CommandOption, Interaction, InteractionType, OptionValue};
pub use invite::Invite;
pub use member::Member;
pub use message::{Attachment, Embed, Message, MessageFlags};
pub use oauth::Scope;
pub use onboarding::OnboardingPrompt;
pub use partials::{Partial, PartialKind};
pub use permissions::{effective, has, Overwrite, Permissions};
pub use poll::{Poll, PollAnswer};
pub use presence::{Activity, Presence, Status, VoiceServerUpdate, VoiceState};
pub use role::Role;
pub use scheduled::ScheduledEvent;
pub use soundboard::SoundboardSound;
pub use stage::StageInstance;
pub use thread::{Thread, ThreadMember, ThreadMetadata};
pub use user::User;
pub use webhook::Webhook;
