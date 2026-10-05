//! Slash-command registration + ACK response (mock-safe, no network).
//!
//! Run: `cargo run -p rivulus --example slash --all-features`
//! (needs the `interactions` + `framework` features; `--all-features` covers it).
//!
//! Live notes (not executed here):
//! - `#[ignore] live`: PUT `bulk_overwrite_payload` output to
//!   `/applications/{app_id}/commands` with `DISCORD_TOKEN`, then answer a
//!   real interaction within the <=3s ACK deadline.

fn main() -> Result<(), rivulus::common::Error> {
    #[cfg(all(feature = "interactions", feature = "framework"))]
    {
        return demo();
    }
    #[cfg(not(all(feature = "interactions", feature = "framework")))]
    {
        eprintln!("run with: cargo run -p rivulus --example slash --all-features");
        return Ok(());
    }
}

#[cfg(all(feature = "interactions", feature = "framework"))]
fn demo() -> Result<(), rivulus::common::Error> {
    use rivulus::interactions::{bulk_overwrite_payload, diff_log, respond, CommandDef};

    // 1. Define + validate (names lowercase, `<= 32` chars).
    let ping = CommandDef::slash("ping", "Reply with pong");
    ping.validate()?;
    let pong = CommandDef::slash("pong", "Reply with ping");
    pong.validate()?;
    let desired = [ping, pong];

    // 2. Bulk-overwrite PUT body (`PUT /applications/{app_id}/commands`).
    let body = bulk_overwrite_payload(&desired);
    println!("bulk payload bytes: {}", body.len());

    // 3. Diff log against the remote set before overwriting.
    let remote = [CommandDef::slash("ping", "Reply with pong")];
    println!("diff:\n{}", diff_log(&remote, &desired));

    // 4. ACK a slash interaction (callback 4; defer when work exceeds 3s).
    let ack = respond("Pong!", false)?;
    let bytes =
        rivulus::common::json::to_vec(&ack).map_err(|e| rivulus::common::Error::Deserialize {
            event: Box::from("respond"),
            reason: rivulus::common::error::truncate_source(&e.to_string()),
        })?;
    println!("ack json: {}", String::from_utf8_lossy(&bytes));

    // 5. Prefix-framework side: parse + guarded registry dispatch.
    let args = rivulus::framework::parse_args("!ping now", "!")?;
    let mut registry = rivulus::framework::Registry::new();
    registry.register(
        "ping",
        std::sync::Arc::new(|_ctx: rivulus::framework::HandlerCtx| {
            println!("framework ping ran");
        }),
    );
    registry.dispatch(&args.name, rivulus::model::Permissions::empty())?;
    println!("registered: {}", registry.len());

    Ok(())
}
