//! Cache tuning presets + background sweeper (mock-safe, no network).
//!
//! Run: `cargo run -p rivulus --example cache-tuning`
//!
//! Live notes (not executed here):
//! - `#[ignore] live`: watch `cache_hit_ratio` under real traffic and move
//!   `minimal` -> `balanced` -> `full` only when the miss rate justifies the
//!   RAM. The RAM table in the root `README.md` is template until P10
//!   measures it.

use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), rivulus::common::Error> {
    use rivulus::cache::sweeper::{Sweeper, SweeperConfig};
    use rivulus::cache::{Cache, CacheConfig, InMemoryCache};

    // Presets are builder fns, not separate types.
    for (name, cfg) in [
        ("minimal", CacheConfig::minimal()),
        ("balanced", CacheConfig::balanced()),
        ("full", CacheConfig::full()),
    ] {
        println!(
            "{name}: types={:?} message_limit={} member_high_water={} sweep_every={}s msg_ttl={}s",
            cfg.resource_types,
            cfg.message_limit,
            cfg.member_high_water,
            cfg.sweep_interval_secs,
            cfg.message_max_age_secs,
        );
    }

    // Wire a preset through the client builder.
    let client = rivulus::Client::builder(String::from("mock-token-for-offline-example"))
        .cache(|_| CacheConfig::balanced())
        .build()?;
    println!(
        "client cache limit: {}",
        client.config().cache.message_limit
    );

    // Background sweeper: sharded `retain`, never blocks reads.
    let cache = Arc::new(InMemoryCache::new(CacheConfig::balanced()));
    let handle = Sweeper::spawn(
        cache.clone(),
        SweeperConfig {
            interval_secs: 60,
            message_max_age_secs: 3600,
        },
    );
    let removed = Sweeper::sweep_once(&cache);
    println!("sweep_once removed {removed} (empty cache: 0)");
    println!("hit_ratio: {:.2}", cache.stats().hit_ratio());
    handle.abort();

    Ok(())
}
