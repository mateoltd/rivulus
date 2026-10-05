// discord.js equivalent bot for P10 RAM comparison (skeleton - see README).
//
// Workload parity with the rivulus side:
//   - same intents (Guilds + GuildMessages + MessageContent)
//   - msg-only vs full-cache presets mirror `cache::CacheConfig`
//   - counts guilds + messages, prints RSS (both sides use /proc/self/status)
//
// Usage: DISCORD_TOKEN=... [PRESET=msg-only|full] node bot.js
// Never run in CI; nightly only. Do NOT commit tokens.

const { Client, GatewayIntentBits } = require('discord.js');

const preset = process.env.PRESET ?? 'msg-only';
const token = process.env.DISCORD_TOKEN;
if (!token) {
  console.error('DISCORD_TOKEN is required');
  process.exit(2);
}

const client = new Client({
  intents: [
    GatewayIntentBits.Guilds,
    GatewayIntentBits.GuildMessages,
    GatewayIntentBits.MessageContent,
  ],
  // PRESET=msg-only disables caches the way rivulus `minimal` does;
  // PRESET=full keeps the default discord.js caches.
  ...(preset === 'msg-only'
    ? { makeCache: () => null }
    : {}),
});

let messages = 0;
client.on('messageCreate', () => {
  messages += 1;
});

client.once('ready', () => {
  const guilds = client.guilds.cache.size;
  const rss = process.memoryUsage().rss;
  console.log(JSON.stringify({ preset, guilds, messages, rss }));
});

setInterval(() => {
  const rss = process.memoryUsage().rss;
  const guilds = client.guilds.cache.size;
  console.log(JSON.stringify({ preset, guilds, messages, rss }));
}, 30_000);

client.login(token);
