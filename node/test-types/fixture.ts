// M3 type fixture: exercises the checked-in index.d.ts surface by type
// only (noEmit never loads the .node binary; ../index.js resolves to the
// adjacent index.d.ts for checking).
import {
  close,
  createClient,
  fetchPage,
  getCacheStats,
  getHealth,
  getLatencies,
  getUptimeMs,
  login,
  streamPages,
  subscribe,
  verifyWebhook,
} from "../index.js";
import type {
  CacheSnapshot,
  HealthSnapshot,
  LatencyRow,
  MessageSnapshot,
} from "../index.js";

async function shapes(): Promise<void> {
  const handle: number = createClient({
    token: "mock",
    intents: 513,
    cache: "balanced",
    sharding: "auto",
  });
  subscribe(handle, ["MESSAGE_CREATE"], (_batch: string): void => {});
  await login(handle);
  const page: MessageSnapshot[] = await fetchPage(handle, "3", 50);
  const first: MessageSnapshot | undefined = page[0];
  if (first) {
    const id: string = first.id;
    void id;
  }
  const pages: MessageSnapshot[][] = await streamPages(handle, "3", 50);
  void pages;
  const health: HealthSnapshot = getHealth(handle);
  void health.ready;
  const rows: LatencyRow[] = getLatencies(handle);
  void rows;
  const uptime: number = getUptimeMs(handle);
  void uptime;
  const cache: CacheSnapshot = getCacheStats(handle);
  void cache.hitRatio;
  verifyWebhook("00", "1", Buffer.alloc(0), "ff");
  await close(handle);
}

void shapes;
