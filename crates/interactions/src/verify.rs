//! Ed25519 webhook signature verification.
//!
//! Discord signs every interaction webhook as `ed25519(timestamp + body)`
//! (`X-Signature-Ed25519` + `X-Signature-Timestamp`). Both headers are
//! required and bodies are capped at 1 MiB before any crypto runs.
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Max interaction webhook body: 1 MiB (larger => [`common::Error::Validation`]).
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Verify `signature = ed25519(timestamp + body)`.
///
/// Both header values are required (empty => [`common::Error::SignatureInvalid`]);
/// bodies over [`MAX_BODY_BYTES`] => [`common::Error::Validation`]; hex-decode
/// or crypto failures => [`common::Error::SignatureInvalid`].
///
/// # Errors
/// Returns [`common::Error::SignatureInvalid`] on missing/bad headers or a bad
/// signature, and [`common::Error::Validation`] when `body` exceeds 1 MiB.
pub fn verify(
    public_key_hex: &str,
    timestamp: &str,
    body: &[u8],
    sig_hex: &str,
) -> Result<(), common::Error> {
    verify_signature(public_key_hex, sig_hex, timestamp, body)
}

/// Verify `signature = ed25519(timestamp + body)` (legacy argument order).
///
/// Same hardening as [`verify`]: empty inputs => [`common::Error::SignatureInvalid`],
/// body over [`MAX_BODY_BYTES`] => [`common::Error::Validation`].
///
/// # Errors
/// Returns [`common::Error::SignatureInvalid`] on missing/bad inputs or a bad
/// signature, and [`common::Error::Validation`] when `body` exceeds 1 MiB.
pub fn verify_signature(
    public_key_hex: &str,
    signature_hex: &str,
    timestamp: &str,
    body: &[u8],
) -> Result<(), common::Error> {
    if public_key_hex.is_empty() || signature_hex.is_empty() || timestamp.is_empty() {
        return Err(common::Error::SignatureInvalid);
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(common::Error::Validation(Box::from(
            "interaction body over 1MiB",
        )));
    }
    let pk = hex::decode(public_key_hex).map_err(|_| common::Error::SignatureInvalid)?;
    let sig = hex::decode(signature_hex).map_err(|_| common::Error::SignatureInvalid)?;
    let key = VerifyingKey::from_bytes(
        pk.as_slice()
            .try_into()
            .map_err(|_| common::Error::SignatureInvalid)?,
    )
    .map_err(|_| common::Error::SignatureInvalid)?;
    let signature = Signature::from_bytes(
        sig.as_slice()
            .try_into()
            .map_err(|_| common::Error::SignatureInvalid)?,
    );
    let mut msg = Vec::with_capacity(timestamp.len() + body.len());
    msg.extend_from_slice(timestamp.as_bytes());
    msg.extend_from_slice(body);
    key.verify(&msg, &signature)
        .map_err(|_| common::Error::SignatureInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair() -> (String, SigningKey) {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk_hex = hex::encode(sk.verifying_key().to_bytes());
        (pk_hex, sk)
    }

    fn signed(sk: &SigningKey, ts: &str, body: &[u8]) -> String {
        let mut msg = Vec::with_capacity(ts.len() + body.len());
        msg.extend_from_slice(ts.as_bytes());
        msg.extend_from_slice(body);
        hex::encode(sk.sign(&msg).to_bytes())
    }

    #[test]
    fn roundtrip() {
        let sk_bytes = [7u8; 32];
        let sk = SigningKey::from_bytes(&sk_bytes);
        let vk = sk.verifying_key();
        let ts = "1234";
        let body = b"hello";
        let mut msg = Vec::new();
        msg.extend_from_slice(ts.as_bytes());
        msg.extend_from_slice(body);
        let sig = sk.sign(&msg);
        let pk_hex = hex::encode(vk.to_bytes());
        let sig_hex = hex::encode(sig.to_bytes());
        assert!(verify_signature(&pk_hex, &sig_hex, ts, body).is_ok());
        assert!(verify_signature(&pk_hex, &sig_hex, "wrong", body).is_err());
    }

    #[test]
    fn verify_ok_and_tamper_fail() {
        let (pk_hex, sk) = keypair();
        let ts = "1699000000";
        let body = deck(&["{\"type\":1}"]);
        let sig_hex = signed(&sk, ts, &body);
        assert!(verify(&pk_hex, ts, &body, &sig_hex).is_ok());
        // Tampered signature (flip the last nibble).
        let mut bad = sig_hex.clone();
        let last = bad.pop().unwrap_or('0');
        bad.push(if last == '0' { '1' } else { '0' });
        assert!(verify(&pk_hex, ts, &body, &bad).is_err());
        // Tampered body.
        assert!(verify(&pk_hex, ts, b"{\"type\":2}", &sig_hex).is_err());
        // Tampered timestamp.
        assert!(verify(&pk_hex, "1699000001", &body, &sig_hex).is_err());
        // Bad hex.
        assert!(verify(&pk_hex, ts, &body, "zz").is_err());
        assert!(verify("zz", ts, &body, &sig_hex).is_err());
    }

    #[test]
    fn headers_required() {
        let (pk_hex, sk) = keypair();
        let ts = "1699000000";
        let body = b"hello";
        let sig_hex = signed(&sk, ts, body);
        assert!(matches!(
            verify("", ts, body, &sig_hex),
            Err(common::Error::SignatureInvalid)
        ));
        assert!(matches!(
            verify(&pk_hex, "", body, &sig_hex),
            Err(common::Error::SignatureInvalid)
        ));
        assert!(matches!(
            verify(&pk_hex, ts, body, ""),
            Err(common::Error::SignatureInvalid)
        ));
    }

    #[test]
    fn body_cap() {
        let (pk_hex, sk) = keypair();
        let ts = "1699000000";
        let big = vec![b'x'; MAX_BODY_BYTES + 1];
        let sig_hex = signed(&sk, ts, &big);
        assert!(matches!(
            verify(&pk_hex, ts, &big, &sig_hex),
            Err(common::Error::Validation(_))
        ));
        // Exactly 1 MiB still verifies.
        let edge = vec![b'y'; MAX_BODY_BYTES];
        let edge_sig = signed(&sk, ts, &edge);
        assert!(verify(&pk_hex, ts, &edge, &edge_sig).is_ok());
    }

    fn deck(parts: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for p in parts {
            out.extend_from_slice(p.as_bytes());
        }
        out
    }
}
