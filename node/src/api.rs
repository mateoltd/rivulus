//! napi surface (plan section 3, pinned by `node/contract.md`).
//!
//! Sync getters never touch the network. Async entry points run on the
//! dedicated runtime installed at module init (see `runtime`).

use std::time::Duration;

use napi::{Env, JsFunction};
use napi_derive::napi;

use crate::errors;
use crate::snapshot::{CacheSnapshot, HealthSnapshot, LatencyRow, MessageSnapshot};
use crate::state;

/// Create options (`createClient` argument; see `node/contract.md`).
#[napi(object)]
pub struct CreateOptions {
    pub token: String,
    pub intents: u32,
    pub cache: String,
    pub sharding: String,
}

/// Validate a numeric handle (integer `u64` in range).
fn check_handle(handle: f64) -> Result<u64, napi::Error> {
    if handle.is_finite() && handle.fract() == 0.0 && handle >= 1.0 {
        Ok(handle as u64)
    } else {
        Err(errors::unknown_handle())
    }
}

/// Look up a live entry or reject with `unknown-handle`.
fn live(handle: f64) -> Result<std::sync::Arc<crate::state::ClientEntry>, napi::Error> {
    let id = check_handle(handle)?;
    state::registry().get(id).ok_or_else(errors::unknown_handle)
}

/// Map a core error to a boundary rejection (parity: same strings).
fn reject(error: rivulus::common::Error) -> napi::Error {
    errors::core(error)
}

/// Create a client; returns its numeric handle.
///
/// The token is kept in Rust as `SecretString` and is never readable back.
/// Only `"auto"` sharding is accepted in M3.
#[napi]
pub fn create_client(options: CreateOptions) -> napi::Result<f64> {
    if options.sharding != "auto" {
        return Err(errors::bad_sharding());
    }
    let token = secrecy::SecretString::from(options.token);
    let id = state::registry()
        .create(token, options.intents, &options.cache, None, None)
        .map_err(reject)?;
    Ok(id as f64)
}

/// Test-only client pointed at mock transports (never production).
///
/// Same contract as `createClient`, plus mock base URLs. Not for live use.
#[napi]
pub fn create_test_client(
    options: CreateOptions,
    rest_base: String,
    gateway_base: String,
) -> napi::Result<f64> {
    if options.sharding != "auto" {
        return Err(errors::bad_sharding());
    }
    let token = secrecy::SecretString::from(options.token);
    let id = state::registry()
        .create(
            token,
            options.intents,
            &options.cache,
            Some(gateway_base),
            Some(rest_base),
        )
        .map_err(reject)?;
    Ok(id as f64)
}

/// Subscribe event kinds with a listener (replaces any previous one).
#[napi]
pub fn subscribe(
    _env: Env,
    handle: f64,
    kinds: Vec<String>,
    listener: JsFunction,
) -> napi::Result<()> {
    let entry = live(handle)?;
    if entry.is_closed() {
        return Err(errors::client_closed());
    }
    state::install_listener(&entry, listener)?;
    entry.set_kinds(kinds);
    Ok(())
}

/// Clear the subscription (releases the listener).
#[napi]
pub fn unsubscribe(handle: f64) -> napi::Result<()> {
    let entry = live(handle)?;
    state::stop_entry(&entry);
    Ok(())
}

/// Login with 30s READY discipline (production or mock gateway per client).
#[napi]
pub async fn login(handle: f64) -> napi::Result<()> {
    let entry = live(handle)?;
    if entry.is_closed() {
        return Err(errors::client_closed());
    }
    let client = std::sync::Arc::clone(&entry.client);
    let gateway_base = entry.gateway_base();
    let login = async move {
        match gateway_base {
            Some(url) => client.login_with_url(&url).await,
            None => client.login().await,
        }
    };
    tokio::time::timeout(Duration::from_secs(30), login)
        .await
        .map_err(|_| errors::err("timeout: login timeout"))?
        .map_err(reject)?;
    if entry.is_closed() {
        return Err(errors::client_closed());
    }
    Ok(())
}

/// Fetch one page of channel history (clamp: `0` maps to `1`).
#[napi]
pub async fn fetch_page(
    handle: f64,
    channel_id: String,
    limit: f64,
) -> napi::Result<Vec<MessageSnapshot>> {
    let entry = live(handle)?;
    if entry.is_closed() {
        return Err(errors::client_closed());
    }
    let channel = channel_id
        .parse::<u64>()
        .ok()
        .and_then(rivulus::model::ChannelId::new)
        .ok_or_else(|| errors::bad_id("channel"))?;
    if !limit.is_finite() || limit.fract() != 0.0 || !(0.0..=255.0).contains(&limit) {
        return Err(errors::bad_id("limit"));
    }
    let capped = (limit as u8).max(1);
    let messenger = rivulus::gateway::ShardMessenger {
        shard: rivulus::gateway::ShardId { id: 0, total: 1 },
        latency: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let ctx = entry.client.context(messenger);
    let page = ctx
        .messages()
        .fetch_page(channel, capped)
        .await
        .map_err(reject)?;
    if entry.is_closed() {
        return Err(errors::client_closed());
    }
    Ok(page.iter().map(|m| MessageSnapshot::of(m)).collect())
}

/// Fetch coarse pages eagerly with a binding-held cursor.
///
/// Stops at the first short page or after 10 pages, whichever comes first.
/// One array crossing per page; no per-item iteration, no retained iterator.
/// (`Page` in the contract is a docs alias for one `Message[]` element.)
#[napi]
pub async fn stream_pages(
    handle: f64,
    channel_id: String,
    limit: f64,
) -> napi::Result<Vec<Vec<MessageSnapshot>>> {
    let entry = live(handle)?;
    if entry.is_closed() {
        return Err(errors::client_closed());
    }
    let channel = channel_id
        .parse::<u64>()
        .ok()
        .and_then(rivulus::model::ChannelId::new)
        .ok_or_else(|| errors::bad_id("channel"))?;
    if !limit.is_finite() || limit.fract() != 0.0 || !(0.0..=255.0).contains(&limit) {
        return Err(errors::bad_id("limit"));
    }
    let capped = (limit as u8).max(1);
    let messenger = rivulus::gateway::ShardMessenger {
        shard: rivulus::gateway::ShardId { id: 0, total: 1 },
        latency: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let ctx = entry.client.context(messenger);
    let mut pages: Vec<Vec<MessageSnapshot>> = Vec::new();
    let mut after: Option<u64> = None;
    for _ in 0u8..10u8 {
        let route = rivulus::rest::Route::ListMessages {
            channel_id: channel.get(),
            limit: u64::from(capped),
            after,
        };
        let bytes = ctx.http.execute(&route, None, None).await.map_err(reject)?;
        let list: Vec<rivulus::model::Message> = serde_json::from_slice(&bytes)
            .map_err(|e| errors::err(format!("deserialize ListMessages: {e}")))?;
        let full = list.len() >= usize::from(capped);
        after = list.last().map(|m| m.id.get());
        pages.push(list.iter().map(MessageSnapshot::of).collect());
        if entry.is_closed() {
            return Err(errors::client_closed());
        }
        if !full {
            break;
        }
    }
    Ok(pages)
}

/// Synchronous health numbers.
#[napi]
pub fn get_health(handle: f64) -> napi::Result<HealthSnapshot> {
    let entry = live(handle)?;
    let health = entry.client.health();
    let first = health.shards.first();
    Ok(HealthSnapshot {
        ready: health.ready,
        shards: health.shards.len() as u32,
        guilds_cached: first.map(|s| s.guilds_cached as u32).unwrap_or(0),
        events_dropped: entry.dropped_count() as u32,
    })
}

/// Synchronous per-shard latencies.
#[napi]
pub fn get_latencies(handle: f64) -> napi::Result<Vec<LatencyRow>> {
    let entry = live(handle)?;
    Ok(entry
        .client
        .latencies()
        .iter()
        .map(|(id, ms)| LatencyRow {
            shard: id.id,
            total: id.total,
            latency_ms: *ms as f64,
        })
        .collect())
}

/// Synchronous uptime in milliseconds (`f64`: JS numbers cannot hold `u64`).
#[napi]
pub fn get_uptime_ms(handle: f64) -> napi::Result<f64> {
    let entry = live(handle)?;
    Ok(entry.client.uptime().as_millis() as f64)
}

/// Synchronous cache numbers.
#[napi]
pub fn get_cache_stats(handle: f64) -> napi::Result<CacheSnapshot> {
    let entry = live(handle)?;
    let messenger = rivulus::gateway::ShardMessenger {
        shard: rivulus::gateway::ShardId { id: 0, total: 1 },
        latency: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let ctx = entry.client.context(messenger);
    let stats = ctx.cache.stats();
    Ok(CacheSnapshot {
        hit_ratio: stats.hit_ratio(),
        guilds: stats.guilds as u32,
        channels: stats.channels as u32,
        messages: stats.messages as u32,
    })
}

/// Pure ed25519 webhook verification (bytes only, fail-closed).
#[napi]
pub fn verify_webhook(
    public_key_hex: String,
    timestamp: String,
    body: napi::bindgen_prelude::Buffer,
    sig_hex: String,
) -> napi::Result<()> {
    let bytes: &[u8] = &body;
    interactions::verify(&public_key_hex, &timestamp, bytes, &sig_hex).map_err(reject)
}

/// Close a client: mark closed, drop the queue, release the listener,/// forget the handle. A second close of the same handle rejects
/// `unknown-handle` (the id no longer maps to anything).
#[napi]
pub async fn close(handle: f64) -> napi::Result<()> {
    let id = check_handle(handle)?;
    match state::registry().remove(id) {
        Some(entry) => {
            entry.mark_closed();
            state::stop_entry(&entry);
            entry.client.shutdown().await;
            Ok(())
        }
        None => Err(errors::unknown_handle()),
    }
}

/// Test-only live-handle count for the leak probe (M4). Never production.
#[napi]
pub fn test_live_count() -> f64 {
    state::registry().live_count() as f64
}
