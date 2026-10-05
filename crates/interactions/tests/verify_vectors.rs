//! Webhook signature vectors: sign-then-verify via `ed25519-dalek`.
use ed25519_dalek::{Signer, SigningKey};

fn keypair() -> (String, SigningKey) {
    let sk = SigningKey::from_bytes(&[9u8; 32]);
    let pk_hex = hex::encode(sk.verifying_key().to_bytes());
    (pk_hex, sk)
}

fn sign_hex(sk: &SigningKey, ts: &str, body: &[u8]) -> String {
    let mut msg = Vec::with_capacity(ts.len() + body.len());
    msg.extend_from_slice(ts.as_bytes());
    msg.extend_from_slice(body);
    hex::encode(sk.sign(&msg).to_bytes())
}

#[test]
fn sign_then_verify_ok() {
    let (pk_hex, sk) = keypair();
    let ts = "1699000000";
    let body = br#"{"type":2,"token":"abc"}"#;
    let sig_hex = sign_hex(&sk, ts, body);
    assert!(interactions::verify::verify(&pk_hex, ts, body, &sig_hex).is_ok());
    assert!(interactions::verify_signature(&pk_hex, &sig_hex, ts, body).is_ok());
}

#[test]
fn tampered_sig_fails() {
    let (pk_hex, sk) = keypair();
    let ts = "1699000000";
    let body = br#"{"type":2}"#;
    let mut sig_hex = sign_hex(&sk, ts, body);
    let last = sig_hex.pop().unwrap_or('0');
    sig_hex.push(if last == '0' { '1' } else { '0' });
    assert!(interactions::verify::verify(&pk_hex, ts, body, &sig_hex).is_err());
}

#[test]
fn tampered_body_fails() {
    let (pk_hex, sk) = keypair();
    let ts = "1699000000";
    let sig_hex = sign_hex(&sk, ts, br#"{"type":2}"#);
    assert!(interactions::verify::verify(&pk_hex, ts, br#"{"type":3}"#, &sig_hex).is_err());
    assert!(
        interactions::verify::verify(&pk_hex, "1699000001", br#"{"type":2}"#, &sig_hex).is_err()
    );
}

#[test]
fn headers_required() {
    let (pk_hex, sk) = keypair();
    let ts = "1699000000";
    let body = br#"{}"#;
    let sig_hex = sign_hex(&sk, ts, body);
    for (pk, t, sig) in [
        ("", ts, sig_hex.as_str()),
        (pk_hex.as_str(), "", sig_hex.as_str()),
        (pk_hex.as_str(), ts, ""),
    ] {
        assert!(
            matches!(
                interactions::verify::verify(pk, t, body, sig),
                Err(common::Error::SignatureInvalid)
            ),
            "pk_empty={} ts_empty={} sig_empty={}",
            pk.is_empty(),
            t.is_empty(),
            sig.is_empty()
        );
    }
    // Non-hex inputs fail as SignatureInvalid, not panic.
    assert!(interactions::verify::verify("not-hex!!", ts, body, &sig_hex).is_err(),);
    assert!(interactions::verify::verify(&pk_hex, ts, body, "not-hex!!").is_err());
}

#[test]
fn body_cap_rejected() {
    let (pk_hex, sk) = keypair();
    let ts = "1699000000";
    let big = vec![b'a'; interactions::MAX_BODY_BYTES + 1];
    let sig_hex = sign_hex(&sk, ts, &big);
    assert!(matches!(
        interactions::verify::verify(&pk_hex, ts, &big, &sig_hex),
        Err(common::Error::Validation(_))
    ));
}
