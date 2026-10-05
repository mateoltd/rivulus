//! `dashmap 6 + foldhash` in-memory cache (locked D4).
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use foldhash::fast::RandomState;

type Map<K, V> = DashMap<K, V, RandomState>;

/// In-memory cache.
///
/// Locks: `dashmap 6 + foldhash` only; values are `Arc<T>`.
/// `users` is canonical (`Map<UserId, Arc<User>>`); `Message.author_id`
/// is resolved via lookup, never duplicated (inline `author` is interned
/// at insert, not stored duplicated).
///
/// Per-channel messages: `VecDeque<MessageId>` capped by `message_limit`
/// (`0` = off, do not store). Oldest evicted first.
///
/// Members: insertion-order eviction per guild above `member_high_water`
/// (`0` = unlimited). This is LRU-ish, not true LRU: no access-order
/// promotion, oldest-inserted evicted first. Documented approximation.
#[derive(Debug)]
pub struct InMemoryCache {
    cfg: crate::CacheConfig,
    guilds: Map<u64, Arc<model::Guild>>,
    channels: Map<u64, Arc<model::Channel>>,
    roles: Map<u64, Arc<model::Role>>,
    users: Map<u64, Arc<model::User>>,
    messages: Map<u64, Arc<model::Message>>,
    members: Map<(u64, u64), Arc<model::Member>>,
    channel_messages: Map<u64, std::sync::Mutex<VecDeque<u64>>>,
    member_order: Map<u64, std::sync::Mutex<VecDeque<u64>>>,
    hit: AtomicU64,
    miss: AtomicU64,
}

impl InMemoryCache {
    /// Build with config.
    #[must_use]
    pub fn new(cfg: crate::CacheConfig) -> Self {
        Self {
            cfg,
            guilds: Map::default(),
            channels: Map::default(),
            roles: Map::default(),
            users: Map::default(),
            messages: Map::default(),
            members: Map::default(),
            channel_messages: Map::default(),
            member_order: Map::default(),
            hit: AtomicU64::new(0),
            miss: AtomicU64::new(0),
        }
    }

    /// Intern a user canonically (entry API, no duplication).
    /// Low-level: no `ResourceType` gate (callers gate on `USER`).
    pub fn intern_user(&self, u: Arc<model::User>) {
        use dashmap::mapref::entry::Entry;
        match self.users.entry(u.id.get()) {
            Entry::Occupied(_) => {}
            Entry::Vacant(v) => {
                v.insert(u);
            }
        }
    }

    fn hit(&self) {
        self.hit.fetch_add(1, Ordering::Relaxed);
    }

    fn miss(&self) {
        self.miss.fetch_add(1, Ordering::Relaxed);
    }

    fn peek_guild(&self, id: model::GuildId) -> Option<Arc<model::Guild>> {
        self.guilds.get(&id.get()).map(|r| r.clone())
    }

    fn peek_channel(&self, id: model::ChannelId) -> Option<Arc<model::Channel>> {
        self.channels.get(&id.get()).map(|r| r.clone())
    }

    fn peek_role(&self, id: model::RoleId) -> Option<Arc<model::Role>> {
        self.roles.get(&id.get()).map(|r| r.clone())
    }

    fn peek_message(&self, id: model::MessageId) -> Option<Arc<model::Message>> {
        self.messages.get(&id.get()).map(|r| r.clone())
    }

    fn peek_member(
        &self,
        guild: model::GuildId,
        user: model::UserId,
    ) -> Option<Arc<model::Member>> {
        self.members
            .get(&(guild.get(), user.get()))
            .map(|r| r.clone())
    }

    fn insert_message_inner(&self, m: Arc<model::Message>) {
        if let Some(a) = &m.author {
            if self.cfg.resource_types.contains(crate::ResourceType::USER) {
                self.intern_user(Arc::new(a.clone()));
            }
        }
        let is_new = !self.messages.contains_key(&m.id.get());
        self.messages.insert(m.id.get(), m.clone());
        if !is_new {
            return;
        }
        let ch = m.channel_id.get();
        let limit = self.cfg.message_limit;
        if limit == 0 {
            return;
        }
        let mid = m.id.get();
        match self.channel_messages.entry(ch) {
            dashmap::mapref::entry::Entry::Occupied(o) => {
                if let Ok(mut d) = o.into_ref().lock() {
                    d.push_back(mid);
                    while d.len() > limit {
                        if let Some(old) = d.pop_front() {
                            self.messages.remove(&old);
                        } else {
                            break;
                        }
                    }
                }
            }
            dashmap::mapref::entry::Entry::Vacant(v) => {
                let mut d = VecDeque::new();
                d.push_back(mid);
                v.insert(std::sync::Mutex::new(d));
            }
        }
    }

    fn remove_message_inner(&self, id: model::MessageId, channel: Option<model::ChannelId>) {
        self.messages.remove(&id.get());
        if let Some(ch) = channel {
            if let Some(d) = self.channel_messages.get(&ch.get()) {
                if let Ok(mut g) = d.lock() {
                    g.retain(|x| *x != id.get());
                }
            }
        } else {
            for r in self.channel_messages.iter() {
                if let Ok(mut g) = r.lock() {
                    g.retain(|x| *x != id.get());
                }
            }
        }
    }

    fn insert_member_inner(&self, m: Arc<model::Member>) {
        if let Some(u) = &m.user {
            if self.cfg.resource_types.contains(crate::ResourceType::USER) {
                self.intern_user(Arc::new(u.clone()));
            }
        }
        let Some(g) = m.guild_id else { return };
        let key = (g.get(), m.user_id.get());
        let is_new = !self.members.contains_key(&key);
        self.members.insert(key, m.clone());
        if !is_new {
            return;
        }
        let high = self.cfg.member_high_water;
        if high == 0 {
            return;
        }
        let uid = m.user_id.get();
        match self.member_order.entry(g.get()) {
            dashmap::mapref::entry::Entry::Occupied(o) => {
                if let Ok(mut d) = o.into_ref().lock() {
                    d.push_back(uid);
                    while d.len() > high {
                        if let Some(old) = d.pop_front() {
                            self.members.remove(&(g.get(), old));
                        } else {
                            break;
                        }
                    }
                }
            }
            dashmap::mapref::entry::Entry::Vacant(v) => {
                let mut d = VecDeque::new();
                d.push_back(uid);
                v.insert(std::sync::Mutex::new(d));
            }
        }
    }

    fn remove_member_inner(&self, guild: model::GuildId, user: model::UserId) {
        self.members.remove(&(guild.get(), user.get()));
        if let Some(o) = self.member_order.get(&guild.get()) {
            if let Ok(mut d) = o.lock() {
                d.retain(|x| *x != user.get());
            }
        }
    }

    /// Sweep messages older than `max_age_secs` (sharded `retain`).
    /// Returns count removed. Never holds a lock across `.await`
    /// (fully synchronous). Used by `sweeper::Sweeper`.
    pub fn sweep_messages(&self, max_age_secs: u64) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let max_age = max_age_secs as i64;
        let mut removed = 0_usize;
        self.messages.retain(|_, v| {
            let age = now.saturating_sub(v.timestamp.unix_timestamp());
            if age > max_age {
                removed = removed.saturating_add(1);
                false
            } else {
                true
            }
        });
        self.channel_messages.retain(|_, d| {
            if let Ok(g) = d.get_mut() {
                g.retain(|id| self.messages.contains_key(id));
                !g.is_empty()
            } else {
                true
            }
        });
        removed
    }
}

impl crate::Cache for InMemoryCache {
    fn config(&self) -> crate::CacheConfig {
        self.cfg.clone()
    }
    fn get_guild(&self, id: model::GuildId) -> Option<Arc<model::Guild>> {
        match self.guilds.get(&id.get()) {
            Some(r) => {
                self.hit();
                Some(r.clone())
            }
            None => {
                self.miss();
                None
            }
        }
    }
    fn get_channel(&self, id: model::ChannelId) -> Option<Arc<model::Channel>> {
        match self.channels.get(&id.get()) {
            Some(r) => {
                self.hit();
                Some(r.clone())
            }
            None => {
                self.miss();
                None
            }
        }
    }
    fn get_role(&self, id: model::RoleId) -> Option<Arc<model::Role>> {
        match self.roles.get(&id.get()) {
            Some(r) => {
                self.hit();
                Some(r.clone())
            }
            None => {
                self.miss();
                None
            }
        }
    }
    fn get_user(&self, id: model::UserId) -> Option<Arc<model::User>> {
        match self.users.get(&id.get()) {
            Some(r) => {
                self.hit();
                Some(r.clone())
            }
            None => {
                self.miss();
                None
            }
        }
    }
    fn get_message(&self, id: model::MessageId) -> Option<Arc<model::Message>> {
        match self.messages.get(&id.get()) {
            Some(r) => {
                self.hit();
                Some(r.clone())
            }
            None => {
                self.miss();
                None
            }
        }
    }
    fn get_member(&self, guild: model::GuildId, user: model::UserId) -> Option<Arc<model::Member>> {
        match self.members.get(&(guild.get(), user.get())) {
            Some(r) => {
                self.hit();
                Some(r.clone())
            }
            None => {
                self.miss();
                None
            }
        }
    }
    fn insert_guild(&self, g: Arc<model::Guild>) {
        if self.cfg.resource_types.contains(crate::ResourceType::GUILD) {
            self.guilds.insert(g.id.get(), g);
        }
    }
    fn insert_channel(&self, c: Arc<model::Channel>) {
        if self
            .cfg
            .resource_types
            .contains(crate::ResourceType::CHANNEL)
        {
            self.channels.insert(c.id.get(), c);
        }
    }
    fn insert_role(&self, r: Arc<model::Role>) {
        if self.cfg.resource_types.contains(crate::ResourceType::ROLE) {
            self.roles.insert(r.id.get(), r);
        }
    }
    fn insert_user(&self, u: Arc<model::User>) {
        if self.cfg.resource_types.contains(crate::ResourceType::USER) {
            self.intern_user(u);
        }
    }
    fn insert_message(&self, m: Arc<model::Message>) {
        if !self
            .cfg
            .resource_types
            .contains(crate::ResourceType::MESSAGE)
        {
            return;
        }
        if self.cfg.message_limit == 0 {
            return;
        }
        self.insert_message_inner(m);
    }
    fn insert_member(&self, m: Arc<model::Member>) {
        if !self
            .cfg
            .resource_types
            .contains(crate::ResourceType::MEMBER)
        {
            return;
        }
        self.insert_member_inner(m);
    }
    fn remove_guild(&self, id: model::GuildId) {
        self.guilds.remove(&id.get());
    }
    fn remove_channel(&self, id: model::ChannelId) {
        self.channels.remove(&id.get());
        self.channel_messages.remove(&id.get());
    }
    fn remove_message(&self, id: model::MessageId) {
        self.remove_message_inner(id, None);
    }
    fn clear(&self) {
        self.guilds.clear();
        self.channels.clear();
        self.roles.clear();
        self.users.clear();
        self.messages.clear();
        self.members.clear();
        self.channel_messages.clear();
        self.member_order.clear();
    }
    fn stats(&self) -> crate::CacheStats {
        crate::CacheStats {
            guilds: self.guilds.len(),
            channels: self.channels.len(),
            roles: self.roles.len(),
            users: self.users.len(),
            members: self.members.len(),
            messages: self.messages.len(),
            hit: self.hit.load(Ordering::Relaxed),
            miss: self.miss.load(Ordering::Relaxed),
        }
    }
    fn update_cache(&self, ev: &model::Event) -> Vec<crate::CacheDiff> {
        match ev {
            model::Event::GuildCreate(g) => {
                if self.cfg.resource_types.contains(crate::ResourceType::GUILD) {
                    self.guilds.insert(g.id.get(), g.clone());
                }
                Vec::new()
            }
            model::Event::GuildAvailable(g) => {
                if self.cfg.resource_types.contains(crate::ResourceType::GUILD) {
                    self.guilds.insert(g.id.get(), g.clone());
                }
                Vec::new()
            }
            model::Event::GuildUpdate(old, new) => {
                if !self.cfg.resource_types.contains(crate::ResourceType::GUILD) {
                    return Vec::new();
                }
                let o = old.clone().or_else(|| self.peek_guild(new.id));
                self.guilds.insert(new.id.get(), new.clone());
                vec![crate::CacheDiff::GuildUpdated {
                    old: o,
                    new: new.clone(),
                }]
            }
            model::Event::GuildDelete { guild_id, .. } => {
                self.guilds.remove(&guild_id.get());
                if let Some((_, order)) = self.member_order.remove(&guild_id.get()) {
                    if let Ok(d) = order.lock() {
                        for uid in d.iter() {
                            self.members.remove(&(guild_id.get(), *uid));
                        }
                    }
                }
                Vec::new()
            }
            model::Event::GuildUnavailable(id) => {
                self.guilds.remove(&id.get());
                Vec::new()
            }
            model::Event::MemberAdd(m) => {
                if self
                    .cfg
                    .resource_types
                    .contains(crate::ResourceType::MEMBER)
                {
                    self.insert_member_inner(m.clone());
                }
                Vec::new()
            }
            model::Event::MemberRemove { guild_id, user_id } => {
                self.remove_member_inner(*guild_id, *user_id);
                Vec::new()
            }
            model::Event::MemberUpdate(old, new) => {
                if !self
                    .cfg
                    .resource_types
                    .contains(crate::ResourceType::MEMBER)
                {
                    return Vec::new();
                }
                let o = match new.guild_id {
                    Some(g) => old.clone().or_else(|| self.peek_member(g, new.user_id)),
                    None => old.clone(),
                };
                self.insert_member_inner(new.clone());
                vec![crate::CacheDiff::MemberUpdated {
                    old: o,
                    new: new.clone(),
                }]
            }
            model::Event::MembersChunk(c) => {
                if self
                    .cfg
                    .resource_types
                    .contains(crate::ResourceType::MEMBER)
                {
                    for m in c.members.iter() {
                        let mut owned = m.clone();
                        if owned.guild_id.is_none() {
                            owned.guild_id = Some(c.guild_id);
                        }
                        self.insert_member_inner(Arc::new(owned));
                    }
                }
                Vec::new()
            }
            model::Event::ChannelCreate(c) => {
                if self
                    .cfg
                    .resource_types
                    .contains(crate::ResourceType::CHANNEL)
                {
                    self.channels.insert(c.id.get(), c.clone());
                }
                Vec::new()
            }
            model::Event::ChannelUpdate(old, new) => {
                if !self
                    .cfg
                    .resource_types
                    .contains(crate::ResourceType::CHANNEL)
                {
                    return Vec::new();
                }
                let o = old.clone().or_else(|| self.peek_channel(new.id));
                self.channels.insert(new.id.get(), new.clone());
                vec![crate::CacheDiff::ChannelUpdated {
                    old: o,
                    new: new.clone(),
                }]
            }
            model::Event::ChannelDelete(c) => {
                self.channels.remove(&c.id.get());
                if let Some((_, deque)) = self.channel_messages.remove(&c.id.get()) {
                    if let Ok(d) = deque.lock() {
                        for mid in d.iter() {
                            self.messages.remove(mid);
                        }
                    }
                }
                Vec::new()
            }
            model::Event::RoleCreate { role, .. } => {
                if self.cfg.resource_types.contains(crate::ResourceType::ROLE) {
                    self.roles.insert(role.id.get(), role.clone());
                }
                Vec::new()
            }
            model::Event::RoleUpdate(old, new) => {
                if !self.cfg.resource_types.contains(crate::ResourceType::ROLE) {
                    return Vec::new();
                }
                let o = old.clone().or_else(|| self.peek_role(new.id));
                self.roles.insert(new.id.get(), new.clone());
                vec![crate::CacheDiff::RoleUpdated {
                    old: o,
                    new: new.clone(),
                }]
            }
            model::Event::RoleDelete { role_id, .. } => {
                self.roles.remove(&role_id.get());
                Vec::new()
            }
            model::Event::MessageCreate(m) => {
                if !self
                    .cfg
                    .resource_types
                    .contains(crate::ResourceType::MESSAGE)
                {
                    return Vec::new();
                }
                if self.cfg.message_limit == 0 {
                    return Vec::new();
                }
                self.insert_message_inner(m.clone());
                Vec::new()
            }
            model::Event::MessageUpdate(old, new) => {
                if !self
                    .cfg
                    .resource_types
                    .contains(crate::ResourceType::MESSAGE)
                {
                    return Vec::new();
                }
                if self.cfg.message_limit == 0 {
                    return Vec::new();
                }
                let o = old.clone().or_else(|| self.peek_message(new.id));
                self.insert_message_inner(new.clone());
                vec![crate::CacheDiff::MessageUpdated {
                    old: o,
                    new: new.clone(),
                }]
            }
            model::Event::MessageDelete {
                channel_id,
                message_id,
            } => {
                self.remove_message_inner(*message_id, Some(*channel_id));
                Vec::new()
            }
            model::Event::MessageDeleteBulk { channel_id, ids } => {
                for id in ids.iter() {
                    self.messages.remove(&id.get());
                }
                if let Some(d) = self.channel_messages.get(&channel_id.get()) {
                    if let Ok(mut g) = d.lock() {
                        g.retain(|x| !ids.iter().any(|id| id.get() == *x));
                    }
                }
                Vec::new()
            }
            model::Event::UserUpdate(u) => {
                if self.cfg.resource_types.contains(crate::ResourceType::USER) {
                    self.intern_user(u.clone());
                }
                Vec::new()
            }
            model::Event::Ready(r) => {
                if self.cfg.resource_types.contains(crate::ResourceType::USER) {
                    self.intern_user(Arc::new(r.user.clone()));
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cache, CacheConfig, ResourceType};

    fn uid(n: u64) -> model::UserId {
        model::UserId::new(n).expect("uid")
    }
    fn gid(n: u64) -> model::GuildId {
        model::GuildId::new(n).expect("gid")
    }
    fn cid(n: u64) -> model::ChannelId {
        model::ChannelId::new(n).expect("cid")
    }
    fn rid(n: u64) -> model::RoleId {
        model::RoleId::new(n).expect("rid")
    }
    fn mid(n: u64) -> model::MessageId {
        model::MessageId::new(n).expect("mid")
    }

    fn full_cfg() -> CacheConfig {
        CacheConfig {
            resource_types: ResourceType::all(),
            message_limit: 10,
            member_high_water: 100,
            ..CacheConfig::default()
        }
    }

    fn user(id: u64) -> Arc<model::User> {
        Arc::new(model::User {
            id: uid(id),
            username: format!("u{id}").into(),
            discriminator: Box::from("0"),
            global_name: None,
            avatar: None,
            bot: false,
            system: false,
        })
    }

    fn guild(id: u64) -> Arc<model::Guild> {
        Arc::new(model::Guild {
            id: gid(id),
            name: format!("g{id}").into(),
            owner_id: uid(1),
            member_count: None,
            roles: Vec::new(),
            emojis: Vec::new(),
            stickers: Vec::new(),
            unavailable: false,
        })
    }

    fn channel(id: u64) -> Arc<model::Channel> {
        Arc::new(model::Channel {
            id: cid(id),
            kind: model::ChannelType::GuildText,
            guild_id: None,
            name: Some(Box::from("c")),
            topic: None,
            position: None,
            permission_overwrites: Vec::new(),
            parent_id: None,
        })
    }

    fn role(id: u64) -> Arc<model::Role> {
        Arc::new(model::Role {
            id: rid(id),
            name: Box::from("r"),
            position: 0,
            permissions: model::Permissions::empty(),
            hoist: false,
            managed: false,
            mentionable: false,
            color: 0,
            flags: 0,
        })
    }

    fn msg(id: u64, ch: u64, author: u64, content: &str, ts: &str) -> Arc<model::Message> {
        let s = format!(
            r#"{{"id":"{id}","channel_id":"{ch}","author":{{"id":"{author}","username":"u"}},"content":"{content}","timestamp":"{ts}"}}"#
        );
        Arc::new(common::json::from_slice::<model::Message>(s.as_bytes()).expect("msg"))
    }

    fn owned_user(id: u64) -> model::User {
        model::User {
            id: uid(id),
            username: format!("u{id}").into(),
            discriminator: Box::from("0"),
            global_name: None,
            avatar: None,
            bot: false,
            system: false,
        }
    }

    fn member(guild_id: Option<u64>, user_id: u64) -> Arc<model::Member> {
        Arc::new(model::Member {
            guild_id: guild_id.map(|g| gid(g)),
            user_id: uid(user_id),
            user: Some(owned_user(user_id)),
            nick: None,
            roles: Vec::new(),
            joined_at: None,
            communication_disabled_until: None,
            deaf: false,
            mute: false,
            pending: false,
        })
    }

    fn fresh_msg(id: u64, ch: u64) -> Arc<model::Message> {
        msg(id, ch, 2, "hi", "2030-01-01T00:00:00Z")
    }

    #[test]
    fn guild_insert_get_remove_with_gate() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c.get_guild(gid(1)).is_none());
        c.insert_guild(guild(1));
        assert_eq!(c.get_guild(gid(1)).expect("guild").name.as_ref(), "g1");
        c.remove_guild(gid(1));
        assert!(c.get_guild(gid(1)).is_none());

        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        off.insert_guild(guild(1));
        assert!(off.get_guild(gid(1)).is_none());
        assert_eq!(off.config().message_limit, 0);
    }

    #[test]
    fn channel_role_user_gating() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c.get_channel(cid(1)).is_none());
        assert!(c.get_role(rid(1)).is_none());
        assert!(c.get_user(uid(1)).is_none());
        c.insert_channel(channel(1));
        c.insert_role(role(1));
        c.insert_user(user(1));
        assert!(c.get_channel(cid(1)).is_some());
        assert!(c.get_role(rid(1)).is_some());
        assert!(c.get_user(uid(1)).is_some());
        c.remove_channel(cid(1));
        assert!(c.get_channel(cid(1)).is_none());

        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        off.insert_channel(channel(1));
        off.insert_role(role(1));
        off.insert_user(user(1));
        assert!(off.get_channel(cid(1)).is_none());
        assert!(off.get_role(rid(1)).is_none());
        assert!(off.get_user(uid(1)).is_none());
    }

    #[test]
    fn user_interning_keeps_first() {
        let c = InMemoryCache::new(full_cfg());
        c.intern_user(user(5));
        let mut second = (*user(5)).clone();
        second.username = Box::from("changed");
        c.intern_user(Arc::new(second));
        assert_eq!(c.get_user(uid(5)).expect("user").username.as_ref(), "u5");
    }

    #[test]
    fn message_author_interned_when_user_enabled() {
        let c = InMemoryCache::new(full_cfg());
        c.insert_message(msg(10, 3, 7, "hi", "2030-01-01T00:00:00Z"));
        assert!(c.get_message(mid(10)).is_some());
        assert_eq!(c.get_user(uid(7)).expect("author").username.as_ref(), "u");

        let no_user = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::MESSAGE,
            message_limit: 10,
            ..CacheConfig::default()
        });
        no_user.insert_message(msg(10, 3, 7, "hi", "2030-01-01T00:00:00Z"));
        assert!(no_user.get_message(mid(10)).is_some());
        assert!(no_user.get_user(uid(7)).is_none());
    }

    #[test]
    fn message_limit_zero_stores_nothing() {
        let c = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::all(),
            message_limit: 0,
            ..CacheConfig::default()
        });
        c.insert_message(fresh_msg(10, 3));
        assert!(c.get_message(mid(10)).is_none());
        // Direct inner call documents the limit-0 early return (no queueing).
        c.insert_message_inner(fresh_msg(11, 3));
        assert!(c.get_message(mid(11)).is_some());
    }

    #[test]
    fn message_gate_off_stores_nothing() {
        let c = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            message_limit: 10,
            ..CacheConfig::default()
        });
        c.insert_message(fresh_msg(10, 3));
        assert!(c.get_message(mid(10)).is_none());
    }

    #[test]
    fn per_channel_eviction_oldest_first() {
        let c = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::all(),
            message_limit: 2,
            ..CacheConfig::default()
        });
        c.insert_message(fresh_msg(1, 3));
        c.insert_message(fresh_msg(2, 3));
        // Re-insert of an existing id must not grow the queue.
        c.insert_message(fresh_msg(1, 3));
        assert_eq!(c.stats().messages, 2);
        c.insert_message(fresh_msg(3, 3));
        assert!(c.get_message(mid(1)).is_none());
        assert!(c.get_message(mid(2)).is_some());
        assert!(c.get_message(mid(3)).is_some());
        assert_eq!(c.stats().messages, 2);
    }

    #[test]
    fn remove_message_with_and_without_channel() {
        let c = InMemoryCache::new(full_cfg());
        c.insert_message(fresh_msg(1, 3));
        c.insert_message(fresh_msg(2, 4));
        c.remove_message_inner(mid(1), Some(cid(3)));
        assert!(c.get_message(mid(1)).is_none());
        assert!(c.get_message(mid(2)).is_some());
        // No channel hint: scan all deques, then re-insert must start fresh.
        c.remove_message(mid(2));
        assert!(c.get_message(mid(2)).is_none());
        c.insert_message(fresh_msg(2, 4));
        assert!(c.get_message(mid(2)).is_some());
    }

    #[test]
    fn member_high_water_evicts_oldest() {
        let c = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::all(),
            member_high_water: 2,
            ..CacheConfig::default()
        });
        c.insert_member(member(Some(9), 1));
        c.insert_member(member(Some(9), 2));
        // Re-insert of an existing key must not grow the order queue.
        c.insert_member(member(Some(9), 1));
        assert_eq!(c.stats().members, 2);
        c.insert_member(member(Some(9), 3));
        assert!(c.get_member(gid(9), uid(1)).is_none());
        assert!(c.get_member(gid(9), uid(2)).is_some());
        assert!(c.get_member(gid(9), uid(3)).is_some());
        // Inline user interned via member insert.
        assert!(c.get_user(uid(3)).is_some());
    }

    #[test]
    fn member_no_guild_never_stored() {
        let c = InMemoryCache::new(full_cfg());
        c.insert_member(member(None, 1));
        assert_eq!(c.stats().members, 0);
    }

    #[test]
    fn member_unlimited_and_gate_off() {
        let c = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::all(),
            member_high_water: 0,
            ..CacheConfig::default()
        });
        for u in 1..=5 {
            c.insert_member(member(Some(9), u));
        }
        assert_eq!(c.stats().members, 5);
        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        off.insert_member(member(Some(9), 1));
        assert_eq!(off.stats().members, 0);
    }

    #[test]
    fn member_remove_prunes_order() {
        let c = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::all(),
            member_high_water: 2,
            ..CacheConfig::default()
        });
        c.insert_member(member(Some(9), 1));
        c.insert_member(member(Some(9), 2));
        c.remove_member_inner(gid(9), uid(1));
        assert!(c.get_member(gid(9), uid(1)).is_none());
        // Slot freed: two more inserts keep both newcomers.
        c.insert_member(member(Some(9), 3));
        c.insert_member(member(Some(9), 4));
        assert!(c.get_member(gid(9), uid(2)).is_none());
        assert!(c.get_member(gid(9), uid(3)).is_some());
        assert!(c.get_member(gid(9), uid(4)).is_some());
    }

    #[test]
    fn stats_hits_misses_and_clear() {
        let c = InMemoryCache::new(full_cfg());
        assert_eq!(c.stats().hit_ratio(), 1.0);
        c.insert_guild(guild(1));
        c.insert_channel(channel(1));
        c.insert_role(role(1));
        c.insert_user(user(1));
        c.insert_message(fresh_msg(1, 1));
        c.insert_member(member(Some(1), 1));
        let s = c.stats();
        // Author `2` is interned alongside the explicit user `1`.
        assert_eq!(
            (s.guilds, s.channels, s.roles, s.messages, s.members),
            (1, 1, 1, 1, 1)
        );
        assert_eq!(s.users, 2);
        assert!(c.get_guild(gid(1)).is_some());
        assert!(c.get_guild(gid(2)).is_none());
        assert!(c.get_channel(cid(1)).is_some());
        assert!(c.get_channel(cid(2)).is_none());
        assert!(c.get_role(rid(1)).is_some());
        assert!(c.get_role(rid(2)).is_none());
        assert!(c.get_user(uid(1)).is_some());
        // Author `2` was interned by the message insert: a hit, not a miss.
        assert!(c.get_user(uid(2)).is_some());
        assert!(c.get_message(mid(1)).is_some());
        assert!(c.get_message(mid(2)).is_none());
        assert!(c.get_member(gid(1), uid(1)).is_some());
        assert!(c.get_member(gid(1), uid(2)).is_none());
        let s = c.stats();
        assert_eq!((s.hit, s.miss), (7, 5));
        assert_eq!(s.hit_ratio(), 7.0 / 12.0);
        c.clear();
        let s = c.stats();
        assert_eq!(
            (s.guilds, s.channels, s.roles, s.messages, s.members),
            (0, 0, 0, 0, 0)
        );
        assert_eq!(s.users, 0);
        assert!(c.get_guild(gid(1)).is_none());
    }

    #[test]
    fn sweep_messages_prunes_and_cleans_deques() {
        let c = InMemoryCache::new(full_cfg());
        c.insert_message(msg(1, 3, 2, "old", "2020-01-01T00:00:00Z"));
        c.insert_message(msg(2, 3, 2, "new", "2030-01-01T00:00:00Z"));
        assert_eq!(c.sweep_messages(60), 1);
        assert!(c.get_message(mid(1)).is_none());
        assert!(c.get_message(mid(2)).is_some());
        // Deque pruned: re-inserting an evicted id starts a fresh entry.
        c.insert_message(msg(3, 3, 2, "new2", "2030-01-01T00:00:00Z"));
        assert_eq!(c.stats().messages, 2);
        assert_eq!(c.sweep_messages(u64::MAX), 0);
    }

    #[test]
    fn update_guild_family() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c
            .update_cache(&model::Event::GuildCreate(guild(1)))
            .is_empty());
        assert!(c.get_guild(gid(1)).is_some());
        assert!(c
            .update_cache(&model::Event::GuildAvailable(guild(2)))
            .is_empty());
        assert!(c.get_guild(gid(2)).is_some());
        // Update with no explicit old falls back to peek.
        let diffs = c.update_cache(&model::Event::GuildUpdate(None, guild(1)));
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], crate::CacheDiff::GuildUpdated { .. }));
        if let crate::CacheDiff::GuildUpdated { old, new } = &diffs[0] {
            assert!(old.is_some());
            assert_eq!(new.id, gid(1));
        }
        // Explicit old wins over peek.
        let diffs = c.update_cache(&model::Event::GuildUpdate(Some(guild(9)), guild(1)));
        if let crate::CacheDiff::GuildUpdated { old, .. } = &diffs[0] {
            assert_eq!(old.as_ref().expect("old").id, gid(9));
        }
        // Gate off: no store, no diff.
        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        assert!(off
            .update_cache(&model::Event::GuildCreate(guild(1)))
            .is_empty());
        assert!(off
            .update_cache(&model::Event::GuildAvailable(guild(1)))
            .is_empty());
        assert!(off
            .update_cache(&model::Event::GuildUpdate(None, guild(1)))
            .is_empty());
        assert!(off.get_guild(gid(1)).is_none());
        // Delete purges per-guild members; unavailable removes the guild.
        c.insert_member(member(Some(1), 1));
        c.insert_member(member(Some(1), 2));
        assert!(c
            .update_cache(&model::Event::GuildDelete {
                guild_id: gid(1),
                unavailable: false
            })
            .is_empty());
        assert!(c.get_guild(gid(1)).is_none());
        assert!(c.get_member(gid(1), uid(1)).is_none());
        assert!(c
            .update_cache(&model::Event::GuildDelete {
                guild_id: gid(2),
                unavailable: true
            })
            .is_empty());
        c.insert_guild(guild(3));
        assert!(c
            .update_cache(&model::Event::GuildUnavailable(gid(3)))
            .is_empty());
        assert!(c.get_guild(gid(3)).is_none());
    }

    #[test]
    fn update_member_family() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c
            .update_cache(&model::Event::MemberAdd(member(Some(1), 1)))
            .is_empty());
        assert!(c.get_member(gid(1), uid(1)).is_some());
        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        assert!(off
            .update_cache(&model::Event::MemberAdd(member(Some(1), 1)))
            .is_empty());
        assert_eq!(off.stats().members, 0);
        assert!(c
            .update_cache(&model::Event::MemberRemove {
                guild_id: gid(1),
                user_id: uid(1)
            })
            .is_empty());
        assert!(c.get_member(gid(1), uid(1)).is_none());
        // Update with peek fallback, then explicit old, then guild-less.
        c.insert_member(member(Some(1), 2));
        let diffs = c.update_cache(&model::Event::MemberUpdate(None, member(Some(1), 2)));
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], crate::CacheDiff::MemberUpdated { .. }));
        let diffs = c.update_cache(&model::Event::MemberUpdate(
            Some(member(Some(1), 9)),
            member(Some(1), 2),
        ));
        if let crate::CacheDiff::MemberUpdated { old, new } = &diffs[0] {
            assert_eq!(old.as_ref().expect("old").user_id, uid(9));
            assert_eq!(new.user_id, uid(2));
        }
        let diffs = c.update_cache(&model::Event::MemberUpdate(None, member(None, 3)));
        assert_eq!(diffs.len(), 1);
        assert!(off
            .update_cache(&model::Event::MemberUpdate(None, member(Some(1), 2)))
            .is_empty());
        // Chunk fills missing guild ids.
        let mut m = (*member(None, 4)).clone();
        let chunk = Arc::new(model::GuildMembersChunk {
            guild_id: gid(1),
            members: vec![m.clone()],
            chunk_index: 0,
            chunk_count: 1,
            nonce: None,
            not_found: Vec::new(),
        });
        m.guild_id = Some(gid(1));
        assert!(c
            .update_cache(&model::Event::MembersChunk(chunk))
            .is_empty());
        assert!(c.get_member(gid(1), uid(4)).is_some());
        assert!(off
            .update_cache(&model::Event::MembersChunk(Arc::new(
                model::GuildMembersChunk {
                    guild_id: gid(1),
                    members: vec![m],
                    chunk_index: 0,
                    chunk_count: 1,
                    nonce: None,
                    not_found: Vec::new(),
                }
            )))
            .is_empty());
    }

    #[test]
    fn update_channel_family() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c
            .update_cache(&model::Event::ChannelCreate(channel(1)))
            .is_empty());
        assert!(c.get_channel(cid(1)).is_some());
        let diffs = c.update_cache(&model::Event::ChannelUpdate(None, channel(1)));
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], crate::CacheDiff::ChannelUpdated { .. }));
        let diffs = c.update_cache(&model::Event::ChannelUpdate(Some(channel(9)), channel(1)));
        if let crate::CacheDiff::ChannelUpdated { old, .. } = &diffs[0] {
            assert_eq!(old.as_ref().expect("old").id, cid(9));
        }
        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        assert!(off
            .update_cache(&model::Event::ChannelCreate(channel(1)))
            .is_empty());
        assert!(off
            .update_cache(&model::Event::ChannelUpdate(None, channel(1)))
            .is_empty());
        // Delete purges the channel's messages.
        c.insert_message(fresh_msg(1, 1));
        assert!(c
            .update_cache(&model::Event::ChannelDelete(channel(1)))
            .is_empty());
        assert!(c.get_channel(cid(1)).is_none());
        assert!(c.get_message(mid(1)).is_none());
    }

    #[test]
    fn update_role_family() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c
            .update_cache(&model::Event::RoleCreate {
                guild_id: gid(1),
                role: role(1)
            })
            .is_empty());
        assert!(c.get_role(rid(1)).is_some());
        let diffs = c.update_cache(&model::Event::RoleUpdate(None, role(1)));
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], crate::CacheDiff::RoleUpdated { .. }));
        let diffs = c.update_cache(&model::Event::RoleUpdate(Some(role(9)), role(1)));
        if let crate::CacheDiff::RoleUpdated { old, .. } = &diffs[0] {
            assert_eq!(old.as_ref().expect("old").id, rid(9));
        }
        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        assert!(off
            .update_cache(&model::Event::RoleCreate {
                guild_id: gid(1),
                role: role(1)
            })
            .is_empty());
        assert!(off
            .update_cache(&model::Event::RoleUpdate(None, role(1)))
            .is_empty());
        assert!(c
            .update_cache(&model::Event::RoleDelete {
                guild_id: gid(1),
                role_id: rid(1)
            })
            .is_empty());
        assert!(c.get_role(rid(1)).is_none());
    }

    #[test]
    fn update_message_family() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c
            .update_cache(&model::Event::MessageCreate(fresh_msg(1, 3)))
            .is_empty());
        assert!(c.get_message(mid(1)).is_some());
        // Author canonically interned through the event path.
        assert!(c.get_user(uid(2)).is_some());
        let diffs = c.update_cache(&model::Event::MessageUpdate(None, fresh_msg(1, 3)));
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], crate::CacheDiff::MessageUpdated { .. }));
        let diffs = c.update_cache(&model::Event::MessageUpdate(
            Some(fresh_msg(9, 3)),
            fresh_msg(1, 3),
        ));
        if let crate::CacheDiff::MessageUpdated { old, new } = &diffs[0] {
            assert_eq!(old.as_ref().expect("old").id, mid(9));
            assert_eq!(new.id, mid(1));
        }
        let gated = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            message_limit: 10,
            ..CacheConfig::default()
        });
        assert!(gated
            .update_cache(&model::Event::MessageCreate(fresh_msg(1, 3)))
            .is_empty());
        assert!(gated
            .update_cache(&model::Event::MessageUpdate(None, fresh_msg(1, 3)))
            .is_empty());
        let no_store = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::all(),
            message_limit: 0,
            ..CacheConfig::default()
        });
        assert!(no_store
            .update_cache(&model::Event::MessageCreate(fresh_msg(1, 3)))
            .is_empty());
        assert!(no_store
            .update_cache(&model::Event::MessageUpdate(None, fresh_msg(1, 3)))
            .is_empty());
        assert!(c
            .update_cache(&model::Event::MessageDelete {
                channel_id: cid(3),
                message_id: mid(1)
            })
            .is_empty());
        assert!(c.get_message(mid(1)).is_none());
        c.insert_message(fresh_msg(2, 3));
        c.insert_message(fresh_msg(3, 3));
        assert!(c
            .update_cache(&model::Event::MessageDeleteBulk {
                channel_id: cid(3),
                ids: vec![mid(2), mid(3)]
            })
            .is_empty());
        assert!(c.get_message(mid(2)).is_none());
        assert!(c.get_message(mid(3)).is_none());
    }

    #[test]
    fn update_user_ready_and_passthrough() {
        let c = InMemoryCache::new(full_cfg());
        assert!(c
            .update_cache(&model::Event::UserUpdate(user(8)))
            .is_empty());
        assert!(c.get_user(uid(8)).is_some());
        let off = InMemoryCache::new(CacheConfig {
            resource_types: ResourceType::empty(),
            ..CacheConfig::default()
        });
        assert!(off
            .update_cache(&model::Event::UserUpdate(user(8)))
            .is_empty());
        assert!(off.get_user(uid(8)).is_none());
        let ready = Arc::new(model::Ready {
            v: 10,
            session_id: Box::from("sess"),
            resume_gateway_url: Box::from("wss://resume/"),
            user: owned_user(9),
        });
        assert!(c.update_cache(&model::Event::Ready(ready)).is_empty());
        assert!(c.get_user(uid(9)).is_some());
        assert!(off
            .update_cache(&model::Event::Ready(Arc::new(model::Ready {
                v: 10,
                session_id: Box::from("sess"),
                resume_gateway_url: Box::from("wss://resume/"),
                user: owned_user(9),
            })))
            .is_empty());
        // Non-cached families pass through with no diffs.
        assert!(c.update_cache(&model::Event::Resumed).is_empty());
        assert!(c
            .update_cache(&model::Event::TypingStart {
                channel_id: cid(1),
                user_id: uid(1)
            })
            .is_empty());
        assert!(c
            .update_cache(&model::Event::ChannelPinsUpdate {
                channel_id: cid(1),
                last_pin: None
            })
            .is_empty());
    }

    #[test]
    fn resource_flags_and_presets() {
        assert!(ResourceType::all().contains(ResourceType::GUILD));
        assert!(CacheConfig::minimal()
            .resource_types
            .contains(ResourceType::GUILD));
        assert!(CacheConfig::balanced()
            .resource_types
            .contains(ResourceType::MEMBER));
        let full = CacheConfig::full();
        assert_eq!(full.message_limit, 50);
        assert!(full.resource_types.contains(ResourceType::MESSAGE));
        let s = crate::CacheStats {
            hit: 3,
            miss: 1,
            ..Default::default()
        };
        assert_eq!(s.hit_ratio(), 0.75);
        assert_eq!(crate::CacheStats::default().hit_ratio(), 1.0);
    }
}
