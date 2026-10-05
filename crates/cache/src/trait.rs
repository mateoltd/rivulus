//! Dyn-safe `Cache` trait (frozen P1a).
use std::sync::Arc;

bitflags::bitflags! {
    /// Which resources the cache stores.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct ResourceType: u64 {
        const GUILD = 1 << 0;
        const CHANNEL = 1 << 1;
        const ROLE = 1 << 2;
        const MEMBER = 1 << 3;
        const USER = 1 << 4;
        const MESSAGE = 1 << 5;
        const REACTION = 1 << 6;
        const VOICE_STATE = 1 << 7;
        const PRESENCE = 1 << 8;
        const THREAD = 1 << 9;
        const THREAD_MEMBER = 1 << 10;
        const STAGE = 1 << 11;
        const SCHEDULED_EVENT = 1 << 12;
        const EMOJI = 1 << 13;
        const STICKER = 1 << 14;
        const INVITE = 1 << 15;
        const SOUNDBOARD = 1 << 16;
        const ENTITLEMENT = 1 << 17;
        const AUTOMOD = 1 << 18;
    }
}

/// Cache configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Enabled resource types.
    pub resource_types: ResourceType,
    /// Per-channel message cap (0 = do not store messages).
    pub message_limit: usize,
    /// Member high-water mark per guild (LRU eviction above).
    pub member_high_water: usize,
    /// Sweeper interval seconds.
    pub sweep_interval_secs: u64,
    /// Message max age seconds.
    pub message_max_age_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            resource_types: ResourceType::GUILD
                | ResourceType::CHANNEL
                | ResourceType::ROLE
                | ResourceType::USER,
            message_limit: 0,
            member_high_water: 10000,
            sweep_interval_secs: 60,
            message_max_age_secs: 3600,
        }
    }
}

impl CacheConfig {
    /// Minimal preset builder fn.
    #[must_use]
    pub fn minimal() -> Self {
        Self::default()
    }
    /// Balanced preset builder fn.
    #[must_use]
    pub fn balanced() -> Self {
        Self {
            resource_types: ResourceType::GUILD
                | ResourceType::CHANNEL
                | ResourceType::ROLE
                | ResourceType::MEMBER
                | ResourceType::USER
                | ResourceType::THREAD
                | ResourceType::VOICE_STATE,
            message_limit: 0,
            ..Self::default()
        }
    }
    /// Full preset builder fn (high RAM, warns).
    #[must_use]
    pub fn full() -> Self {
        Self {
            resource_types: ResourceType::all(),
            message_limit: 50,
            ..Self::default()
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    /// Counts.
    pub guilds: usize,
    /// Channels.
    pub channels: usize,
    /// Roles.
    pub roles: usize,
    /// Users.
    pub users: usize,
    /// Members.
    pub members: usize,
    /// Messages.
    pub messages: usize,
    /// Hits.
    pub hit: u64,
    /// Misses.
    pub miss: u64,
}

impl CacheStats {
    /// Hit ratio 0.0-1.0.
    #[must_use]
    pub fn hit_ratio(self) -> f64 {
        let t = self.hit + self.miss;
        if t == 0 {
            1.0
        } else {
            self.hit as f64 / t as f64
        }
    }
}

/// Diff emitted by `update_cache` (Arc-clone old before insert, never full-guild clone).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CacheDiff {
    /// Guild updated.
    GuildUpdated {
        /// Old.
        old: Option<Arc<model::Guild>>,
        /// New.
        new: Arc<model::Guild>,
    },
    /// Channel updated.
    ChannelUpdated {
        /// Old.
        old: Option<Arc<model::Channel>>,
        /// New.
        new: Arc<model::Channel>,
    },
    /// Role updated.
    RoleUpdated {
        /// Old.
        old: Option<Arc<model::Role>>,
        /// New.
        new: Arc<model::Role>,
    },
    /// Member updated.
    MemberUpdated {
        /// Old.
        old: Option<Arc<model::Member>>,
        /// New.
        new: Arc<model::Member>,
    },
    /// Message updated.
    MessageUpdated {
        /// Old.
        old: Option<Arc<model::Message>>,
        /// New.
        new: Arc<model::Message>,
    },
}

/// Dyn-safe cache trait (sync fns only, no generics, no `impl Stream`).
pub trait Cache: Send + Sync + 'static {
    /// Config snapshot.
    fn config(&self) -> CacheConfig;
    /// Get guild.
    fn get_guild(&self, id: model::GuildId) -> Option<Arc<model::Guild>>;
    /// Get channel.
    fn get_channel(&self, id: model::ChannelId) -> Option<Arc<model::Channel>>;
    /// Get role.
    fn get_role(&self, id: model::RoleId) -> Option<Arc<model::Role>>;
    /// Get user (canonical interned).
    fn get_user(&self, id: model::UserId) -> Option<Arc<model::User>>;
    /// Get message.
    fn get_message(&self, id: model::MessageId) -> Option<Arc<model::Message>>;
    /// Get member.
    fn get_member(&self, guild: model::GuildId, user: model::UserId) -> Option<Arc<model::Member>>;
    /// Insert guild.
    fn insert_guild(&self, g: Arc<model::Guild>);
    /// Insert channel.
    fn insert_channel(&self, c: Arc<model::Channel>);
    /// Insert role.
    fn insert_role(&self, r: Arc<model::Role>);
    /// Insert user (canonical).
    fn insert_user(&self, u: Arc<model::User>);
    /// Insert message.
    fn insert_message(&self, m: Arc<model::Message>);
    /// Insert member.
    fn insert_member(&self, m: Arc<model::Member>);
    /// Remove guild.
    fn remove_guild(&self, id: model::GuildId);
    /// Remove channel.
    fn remove_channel(&self, id: model::ChannelId);
    /// Remove message.
    fn remove_message(&self, id: model::MessageId);
    /// Clear all.
    fn clear(&self);
    /// Stats.
    fn stats(&self) -> CacheStats;
    /// Apply a dispatch event (gated by `ResourceType`), returning diffs.
    fn update_cache(&self, ev: &model::Event) -> Vec<CacheDiff>;
}
