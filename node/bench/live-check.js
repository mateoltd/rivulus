// M6 live connectivity harness (LOCAL ONLY, never CI).
//
// Read-only use of the Node binding against live Discord: login plus one
// fetchPage on the quiet guild, health/latencies/caches read out. No
// writes. Refuses to run under CI. Token comes from DISCORD_TOKEN env
// only, is never printed, and never appears in the evidence file: the
// only logged values are guild/channel/message COUNTS plus error strings.
// IDs, names, and the token stay in memory.
if (process.env.CI || process.env.GITHUB_ACTIONS) {
  console.error("live-check: refuses to run under CI (local-only harness)");
  process.exit(2);
}

const fs = require("node:fs");
const path = require("node:path");
const binding = require("../index.js");

const API = "https://discord.com/api/v10";

async function api(token, route) {
  const response = await fetch(`${API}${route}`, {
    headers: { Authorization: `Bot ${token}` },
  });
  if (response.status === 401 || response.status === 403) {
    throw new Error(`auth ${response.status}`);
  }
  if (!response.ok) {
    throw new Error(`rest ${response.status}`);
  }
  return response.json();
}

async function main() {
  const started = Date.now();
  const token = process.env.DISCORD_TOKEN;
  if (!token) {
    console.error("live-check: set DISCORD_TOKEN to run (local only)");
    process.exit(2);
  }
  const evidence = {
    date: new Date().toISOString().slice(0, 10),
    ready: false,
    loginElapsedMs: 0,
    guilds: 0,
    channels: 0,
    messages: 0,
    healthReady: false,
    shards: 0,
    latencies: [],
    hitRatio: 0,
    error: null,
  };
  try {
    const guilds = await api(token, "/users/@me/guilds?limit=10");
    evidence.guilds = guilds.length;
    const firstGuild = guilds[0];
    const channels = await api(token, `/guilds/${firstGuild.id}/channels`);
    evidence.channels = channels.length;
    const textish = channels.find((c) => c.type === 0) || channels[0];
    const loginStart = Date.now();
    const handle = binding.createClient({
      token,
      intents: 513,
      cache: "balanced",
      sharding: "auto",
    });
    try {
      await binding.login(handle);
      evidence.ready = true;
      evidence.loginElapsedMs = Date.now() - loginStart;
      const page = await binding.fetchPage(handle, textish.id, 5);
      evidence.messages = page.length;
      const health = binding.getHealth(handle);
      evidence.healthReady = health.ready;
      evidence.shards = health.shards;
      evidence.latencies = binding.getLatencies(handle).map((row) => ({
        shard: row.shard,
        total: row.total,
        latencyMs: row.latencyMs,
      }));
      evidence.hitRatio = binding.getCacheStats(handle).hitRatio;
      await binding.close(handle);
    } catch (inner) {
      try {
        await binding.close(handle);
      } catch {
        // Already closed or unknown; original error stands.
      }
      throw inner;
    }
  } catch (error) {
    evidence.error = String((error && error.message) || error);
  }
  evidence.elapsedMs = Date.now() - started;
  const outPath = path.join(
    __dirname,
    "..",
    "..",
    "bench",
    "results",
    `${evidence.date}-node-live.json`
  );
  fs.writeFileSync(outPath, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(JSON.stringify(evidence));
  if (evidence.error && !evidence.ready) {
    process.exit(1);
  }
}

main();
