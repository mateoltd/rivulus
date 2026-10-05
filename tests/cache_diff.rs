//! Cache diff integration: old/new `Arc` without full-guild clone.
use std::sync::Arc;

use cache::{Cache, CacheConfig, InMemoryCache, ResourceType};

fn guild(id: u64, name: &str) -> Arc<model::Guild> {
    Arc::new(model::Guild {
        id: model::GuildId::new(id).unwrap(),
        name: Box::from(name),
        owner_id: model::UserId::new(2).unwrap(),
        member_count: None,
        roles: Vec::new(),
        emojis: Vec::new(),
        stickers: Vec::new(),
        unavailable: false,
    })
}

fn msg_json(id: &str, channel: &str, content: &str, ts: &str) -> model::Message {
    let v = format!(
        r#"{{"id":"{id}","channel_id":"{channel}","author_id":"2","author":{{"id":"2","username":"u","discriminator":"0"}},"content":"{content}","timestamp":"{ts}"}}"#
    );
    common::json::from_slice::<model::Message>(v.as_bytes()).unwrap()
}

fn full_cfg() -> CacheConfig {
    CacheConfig {
        resource_types: ResourceType::GUILD
            | ResourceType::CHANNEL
            | ResourceType::ROLE
            | ResourceType::USER
            | ResourceType::MEMBER
            | ResourceType::MESSAGE,
        message_limit: 10,
        member_high_water: 10000,
        ..CacheConfig::default()
    }
}

#[test]
fn guild_update_diff_is_arc_clone() {
    let c = InMemoryCache::new(full_cfg());
    let g1 = guild(1, "a");
    let g2 = guild(1, "b");
    assert!(c.update_cache(&model::Event::GuildCreate(g1.clone())).is_empty());
    let diffs = c.update_cache(&model::Event::GuildUpdate(None, g2.clone()));
    assert_eq!(diffs.len(), 1);
    match &diffs[0] {
        cache::CacheDiff::GuildUpdated { old, new } => {
            let old = old.as_ref().expect("old present");
            assert!(Arc::ptr_eq(old, &g1), "old must be Arc-clone, no full-guild clone");
            assert!(Arc::ptr_eq(new, &g2));
            assert_eq!(old.name.as_ref(), "a");
            assert_eq!(new.name.as_ref(), "b");
        }
        _ => panic!("expected GuildUpdated"),
    }
}

#[test]
fn message_update_diff_is_arc_clone() {
    let c = InMemoryCache::new(full_cfg());
    let m1 = Arc::new(msg_json("10", "3", "hi", "2030-01-01T00:00:00Z"));
    let m2 = Arc::new(msg_json("10", "3", "edited", "2030-01-01T00:01:00Z"));
    assert!(c.update_cache(&model::Event::MessageCreate(m1.clone())).is_empty());
    let diffs = c.update_cache(&model::Event::MessageUpdate(None, m2.clone()));
    assert_eq!(diffs.len(), 1);
    match &diffs[0] {
        cache::CacheDiff::MessageUpdated { old, new } => {
            let old = old.as_ref().expect("old present");
            assert!(Arc::ptr_eq(old, &m1));
            assert!(Arc::ptr_eq(new, &m2));
            assert_eq!(old.content.as_ref(), "hi");
            assert_eq!(new.content.as_ref(), "edited");
        }
        _ => panic!("expected MessageUpdated"),
    }
}

#[test]
fn users_canonical_no_duplicate() {
    let c = InMemoryCache::new(full_cfg());
    let m1 = Arc::new(msg_json("20", "3", "one", "2030-01-01T00:00:00Z"));
    c.update_cache(&model::Event::MessageCreate(m1));
    let uid = model::UserId::new(2).unwrap();
    let u = c.get_user(uid).expect("author interned");
    assert_eq!(u.username.as_ref(), "u");
    assert_eq!(c.stats().users, 1);
}

#[test]
fn channel_deque_evicts_oldest() {
    let cfg = CacheConfig {
        resource_types: ResourceType::MESSAGE | ResourceType::USER,
        message_limit: 2,
        ..CacheConfig::default()
    };
    let c = InMemoryCache::new(cfg);
    for (id, content) in [("30", "a"), ("31", "b"), ("32", "c")] {
        let m = Arc::new(msg_json(id, "7", content, "2030-01-01T00:00:00Z"));
        c.update_cache(&model::Event::MessageCreate(m));
    }
    assert_eq!(c.stats().messages, 2);
    assert!(c.get_message(model::MessageId::new(30).unwrap()).is_none());
    assert!(c.get_message(model::MessageId::new(31).unwrap()).is_some());
    assert!(c.get_message(model::MessageId::new(32).unwrap()).is_some());
}

#[test]
fn stats_hit_miss() {
    let c = InMemoryCache::new(full_cfg());
    let gid = model::GuildId::new(9).unwrap();
    assert!(c.get_guild(gid).is_none());
    assert_eq!(c.stats().miss, 1);
    assert_eq!(c.stats().hit, 0);
    c.update_cache(&model::Event::GuildCreate(guild(9, "g")));
    assert!(c.get_guild(gid).is_some());
    let s = c.stats();
    assert_eq!(s.hit, 1);
    assert_eq!(s.miss, 1);
    assert!((s.hit_ratio() - 0.5).abs() < f64::EPSILON);
}
