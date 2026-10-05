//! Minimal ping bot (mock-safe: builds + dispatches offline, no login).
//!
//! Run: `cargo run -p rivulus --example ping`
//!
//! Live notes (not executed here):
//! - `#[ignore] live`: set `DISCORD_TOKEN`, then `client.login().await`
//!   followed by `client.shutdown().await` on Ctrl-C.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use futures::StreamExt;

/// Struct handler (first of several listeners on one client).
struct Ping;

impl rivulus::EventHandler for Ping {
    fn on_dispatch(
        &self,
        _ctx: rivulus::Context,
        ev: rivulus::model::Event,
    ) -> futures::future::BoxFuture<'_, ()> {
        Box::pin(async move {
            if matches!(ev, rivulus::model::Event::MessageCreate(_)) {
                println!("ping: message seen");
            }
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), rivulus::common::Error> {
    // Sync builder: token + intents + cache + sharding (no I/O in `build`).
    let client = rivulus::Client::builder(String::from("mock-token-for-offline-example"))
        .intents(rivulus::model::Intents::GUILDS | rivulus::model::Intents::GUILD_MESSAGES)
        .cache(|_| rivulus::cache::CacheConfig::minimal())
        .sharding(rivulus::gateway::ShardStrategy::Auto)
        .add_handler(Ping)
        .add_handler(
            |_ctx: rivulus::Context, ev: rivulus::model::Event| async move {
                println!("closure handler saw {}", ev.kind());
            },
        )
        .build()?;

    println!("shards: {:?}", client.latencies());
    println!("uptime: {:?}", client.uptime());
    println!("health ready: {}", client.health().ready);

    // Mock context (no login): managers work cache-first offline.
    let messenger = rivulus::gateway::ShardMessenger {
        shard: rivulus::gateway::ShardId { id: 0, total: 1 },
        latency: Arc::new(AtomicU64::new(0)),
    };
    let ctx = client.context(messenger);

    let Some(channel) = rivulus::model::ChannelId::new(3) else {
        return Err(rivulus::common::Error::Validation(Box::from(
            "bad mock channel id",
        )));
    };
    // Dual pagination API: fetch one page AND stream pages.
    let page = ctx.messages().fetch_page(channel, 50).await?;
    println!(
        "fetch_page: {} messages (mock: empty until ListMessages lands)",
        page.len()
    );
    let pages = ctx.messages().stream_pages(channel, 50);
    tokio::pin!(pages);
    while let Some(page) = pages.next().await {
        println!("stream_pages item: {} messages", page?.len());
    }

    // Live (`#[ignore] live`, needs `DISCORD_TOKEN`): `client.login().await`.
    Ok(())
}
