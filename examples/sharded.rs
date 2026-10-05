//! Sharded client sketch (mock-safe: pure bucket math + builder, no login).
//!
//! Run: `cargo run -p rivulus --example sharded`
//!
//! Live notes (not executed here):
//! - `#[ignore] live`: fetch `GET /gateway/bot` for the real
//!   `shards`/`max_concurrency`, then `client.login().await` to spawn the
//!   `Cluster` in identify-bucket order.

#[tokio::main]
async fn main() -> Result<(), rivulus::common::Error> {
    use rivulus::gateway::{
        bucket, connect_url, ordered_start_list, recommended_shards, shard_ids, ClusterConfig,
        ShardStrategy,
    };

    // Pure `GET /gateway/bot` shape (no rest dep: gateway stays IO-free here).
    let bot = rivulus::model::GetGatewayBotResponse {
        url: Box::from("wss://gateway.discord.gg"),
        shards: 4,
        session_start_limit: rivulus::model::SessionStartLimit {
            total: 1000,
            remaining: 999,
            reset_after: 1,
            max_concurrency: 2,
        },
    };
    let total = recommended_shards(&bot);
    let max_concurrency = u32::try_from(bot.session_start_limit.max_concurrency).map_err(|_| {
        rivulus::common::Error::Validation(Box::from("max_concurrency overflows u32"))
    })?;
    let config = ClusterConfig::new(total, max_concurrency);
    println!("total shards: {total}, config: {config:?}");

    // Identify-bucket math: `bucket(shard) = shard % max_concurrency`,
    // one identify per 5s per bucket.
    let order = ordered_start_list(config.total, config.max_concurrency);
    println!("ordered start: {order:?}");
    for id in &order {
        println!(
            "shard {id} -> bucket {}",
            bucket(*id, config.max_concurrency)
        );
    }
    println!("connect: {}", connect_url("wss://gateway.discord.gg"));
    println!("shard ids: {:?}", shard_ids(&config));

    // Builder carries the strategy; the `Cluster` spawns at `login()`.
    let client = rivulus::Client::builder(String::from("mock-token-for-offline-example"))
        .intents(rivulus::model::Intents::GUILDS | rivulus::model::Intents::GUILD_MESSAGES)
        .sharding(ShardStrategy::Auto)
        .build()?;
    println!("strategy: {:?}", client.config().sharding);
    println!("placeholder shards: {:?}", client.latencies());

    Ok(())
}
