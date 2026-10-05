// ed25519 verify round-trip with real keys (node:crypto). No fixtures,
// no secrets beyond ephemeral test keys.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const binding = require("../index.js");

function keypair() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync("ed25519");
  const jwk = publicKey.export({ format: "jwk" });
  const pubHex = Buffer.from(jwk.x, "base64url").toString("hex");
  return { publicKey, privateKey, pubHex };
}

test("valid signature verifies", () => {
  const { privateKey, pubHex } = keypair();
  const timestamp = "1699000000";
  const body = Buffer.from('{"type":1}');
  const sig = crypto.sign(null, Buffer.concat([Buffer.from(timestamp), body]), privateKey);
  binding.verifyWebhook(pubHex, timestamp, body, sig.toString("hex"));
});

test("tampered body fails closed with the typed string", () => {
  const { privateKey, pubHex } = keypair();
  const timestamp = "1699000000";
  const body = Buffer.from('{"type":1}');
  const sig = crypto.sign(null, Buffer.concat([Buffer.from(timestamp), body]), privateKey);
  assert.throws(
    () =>
      binding.verifyWebhook(
        pubHex,
        timestamp,
        Buffer.from('{"type":2}'),
        sig.toString("hex")
      ),
    /signature invalid/
  );
});

test("malformed hex fails with its own typed error", () => {
  assert.throws(
    () =>
      binding.verifyWebhook("zz", "1699000000", Buffer.alloc(0), "ff"),
    /signature invalid/
  );
});
