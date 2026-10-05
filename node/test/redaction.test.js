// Redaction: the mock secret must not surface in outputs or errors,
// verbatim or base64-transformed, on happy or failure paths.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const binding = require("../index.js");
const { SECRET, message, setup, teardown } = require("./helpers/fixture");

function leaks(text) {
  if (typeof text !== "string") {
    return false;
  }
  if (text.includes(SECRET)) {
    return true;
  }
  const encoded = Buffer.from(SECRET).toString("base64");
  return text.includes(encoded);
}

test("happy-path outputs carry no secret", async (t) => {
  const ctx = await setup([[message("4")]], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  const page = await binding.fetchPage(ctx.handle, "3", 50);
  assert.ok(!leaks(JSON.stringify(page)));
  const health = binding.getHealth(ctx.handle);
  assert.ok(!leaks(JSON.stringify(health)));
  await teardown(ctx);
});

test("failure-path errors carry no secret", async (t) => {
  const ctx = await setup([], {});
  t.after(() => teardown(ctx));
  const errors = [];
  try {
    await binding.fetchPage(ctx.handle, "3", 5);
  } catch (error) {
    errors.push(String((error && error.message) || error));
  }
  try {
    await binding.login(999999);
  } catch (error) {
    errors.push(String((error && error.message) || error));
  }
  for (const text of errors) {
    assert.ok(!leaks(text), `leak in: ${text.slice(0, 80)}`);
  }
  await teardown(ctx);
});
