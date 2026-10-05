//! Interaction webhook signature verification (pure crypto, no server).
//!
//! Run: `cargo run -p rivulus --example webhook-verify-axum --all-features`
//! (needs the `interactions` feature; `--all-features` covers it).
//!
//! NOTE: this example deliberately has NO axum dependency. The `verify()`
//! call below is the whole integration: axum (or any server) wiring is user
//! code - extract `X-Signature-Ed25519` + `X-Signature-Timestamp`, call
//! `verify()`, and only then parse the body. A minimal axum sketch follows
//! as a comment (do not paste it without adding `axum` to YOUR crate;
//! `interactions-axum` is example-only and NOT a `rivulus` feature, v1).

// ```text
// // User code (requires YOUR OWN `axum` dependency):
// async fn discord_webhook(headers, raw_body) {
//     rivulus::interactions::verify(PUBLIC_KEY_HEX, &timestamp, &body, &sig)?;
//     let interaction: serde_json::Value = serde_json::from_slice(&body)?;
//     if interaction["type"] == 1 { /* reply {"type": 1} (Pong) */ }
// }
// ```

fn main() -> Result<(), rivulus::common::Error> {
    #[cfg(feature = "interactions")]
    {
        return demo();
    }
    #[cfg(not(feature = "interactions"))]
    {
        eprintln!("run with: cargo run -p rivulus --example webhook-verify-axum --all-features");
        return Ok(());
    }
}

#[cfg(feature = "interactions")]
fn demo() -> Result<(), rivulus::common::Error> {
    // A bogus key fails closed with `SignatureInvalid` (never accept on error).
    let body = br#"{"type":1}"#;
    let res = rivulus::interactions::verify(
        "00".repeat(32).as_str(),
        "1699000000",
        body,
        "ff".repeat(64).as_str(),
    );
    let err = match res {
        Ok(()) => {
            return Err(rivulus::common::Error::Validation(Box::from(
                "bogus signature verified",
            )));
        }
        Err(e) => e,
    };
    assert!(matches!(err, rivulus::common::Error::SignatureInvalid));
    println!("bogus signature rejected: {err}");

    // Missing headers fail the same closed way (empty => SignatureInvalid).
    assert!(rivulus::interactions::verify("", "1699000000", body, "ff").is_err());
    // Bodies over 1 MiB are rejected before any crypto runs.
    let big = vec![b'x'; rivulus::interactions::MAX_BODY_BYTES + 1];
    assert!(matches!(
        rivulus::interactions::verify("aa", "t", &big, "bb"),
        Err(rivulus::common::Error::Validation(_))
    ));
    println!("header + body-cap hardening ok");

    Ok(())
}
