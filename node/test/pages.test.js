// Pages against mocks: snapshots, clamp, cursor advance. Mock only.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const binding = require("../index.js");
const { message, setup, teardown } = require("./helpers/fixture");

test("fetchPage returns snapshots with string ids", async (t) => {
  const ctx = await setup([[message("4")]], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  const page = await binding.fetchPage(ctx.handle, "3", 50);
  assert.equal(page.length, 1);
  assert.equal(page[0].id, "4");
  assert.equal(page[0].channelId, "3");
  assert.equal(page[0].content, "hi");
  assert.ok(page[0].timestamp.length > 0);
  await teardown(ctx);
});

test("clamp: limit 0 maps to 1", async (t) => {
  const ctx = await setup([[message("9")]], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  const page = await binding.fetchPage(ctx.handle, "3", 0);
  assert.equal(page.length, 1);
  await teardown(ctx);
});

test("streamPages advances by cursor then stops on a short page", async (t) => {
  const ctx = await setup([[message("4")], []], {});
  t.after(() => teardown(ctx));
  await binding.login(ctx.handle);
  const pages = await binding.streamPages(ctx.handle, "3", 1);
  assert.equal(pages.length, 2);
  assert.equal(pages[0].length, 1);
  assert.equal(pages[1].length, 0);
  await teardown(ctx);
});
