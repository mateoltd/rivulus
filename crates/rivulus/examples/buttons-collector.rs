//! Button/collector example over a mock event feed (no network).
//!
//! Run: `cargo run -p rivulus --example buttons-collector --all-features`
//! (needs the `standby` feature; `--all-features` covers it).
//!
//! Live notes (not executed here):
//! - `#[ignore] live`: replace the mock `feed` below with the real
//!   dispatcher (`client.dispatch(&ctx, ev).await` feeds `standby`), and
//!   filter on `InteractionCreate` with a matching `custom_id`.

#[tokio::main]
async fn main() -> Result<(), rivulus::common::Error> {
    #[cfg(feature = "standby")]
    {
        return demo().await;
    }
    #[cfg(not(feature = "standby"))]
    {
        eprintln!("run with: cargo run -p rivulus --example buttons-collector --all-features");
        return Ok(());
    }
}

#[cfg(feature = "standby")]
async fn demo() -> Result<(), rivulus::common::Error> {
    use std::sync::Arc;
    use std::time::Duration;

    use rivulus::standby::{Collector, CollectorConfig, Standby, WaiterKind};

    // 1. `wait_for` on a mock event: feed one side, await the other.
    let standby = Arc::new(Standby::new());
    let feeder = standby.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        feeder.feed(Arc::new(rivulus::model::Event::Resumed));
    });
    let ev = standby
        .wait_for(WaiterKind::Any, Duration::from_secs(5), |_| true)
        .await?;
    println!("wait_for got {}", ev.kind());

    // 2. Collector with filter + max + time + idle (discord.js semantics).
    let mut collector = Collector::with_filter(
        CollectorConfig {
            max: Some(2),
            time: Duration::from_secs(60),
            idle: Some(Duration::from_secs(30)),
        },
        |_| true,
    );
    let mock = Arc::new(rivulus::model::Event::Resumed);
    assert_eq!(collector.push(mock.clone()), None);
    let end = collector.push(mock);
    println!("end_reason: {end:?} (Limit at max = 2)");
    println!("collected: {}", collector.items().len());

    // 3. Delete events auto-end the collector.
    let Some(channel) = rivulus::model::ChannelId::new(3) else {
        return Err(rivulus::common::Error::Validation(Box::from(
            "bad mock channel id",
        )));
    };
    let Some(message) = rivulus::model::MessageId::new(4) else {
        return Err(rivulus::common::Error::Validation(Box::from(
            "bad mock message id",
        )));
    };
    let mut deleter = Collector::new(CollectorConfig::default());
    let end = deleter.push(Arc::new(rivulus::model::Event::MessageDelete {
        channel_id: channel,
        message_id: message,
    }));
    println!("delete end_reason: {end:?} (MessageDelete)");
    println!("collector end_reason(): {:?}", deleter.end_reason());

    Ok(())
}
