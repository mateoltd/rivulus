# djs-equiv (nightly skeleton — not run in CI)

Identical-workload discord.js bot for the P10 RAM comparison (plans/05 §4).

## Setup (Node 20 + latest discord.js)

```sh
node --version  # must be >= 20
cd bench/djs-equiv
npm ci            # installs discord.js ^14 (do NOT run node here per P10 task)
```

## Run (nightly only)

```sh
DISCORD_TOKEN=... PRESET=msg-only node bot.js   # minimal caches
DISCORD_TOKEN=... PRESET=full node bot.js       # default caches
```

`PRESET` mirrors `cache::CacheConfig::minimal()` / `full()`.
The bot logs `{ preset, guilds, messages, rss }` JSON lines every 30s.

## Measuring RSS (both sides identically)

Read `VmRSS` from `/proc/self/status` at matched guild counts:

```sh
grep VmRSS /proc/<pid>/status
```

Matrix: `quick` = 100 guilds (CI-adjacent, rivulus side only);
nightly = 1k / 10k / 50k guilds x msg-only / full-cache.
Paste both sides into `bench/results/<date>.md`.
Gate: rivulus RSS `<= 20%` of discord.js RSS **with tolerance bands**
(`bench/compare.py`), never absolute RSS.
