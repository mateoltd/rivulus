// Scripted mock gateway (WebSocket) for M4 boundary tests (127.0.0.1).
// Sequence per connection: Hello -> expect Identify -> Ready (+ optional
// extra frames) -> ACK every heartbeat. `silent: true` accepts and then
// never answers (login-timeout path). Mock only; never production.
const { WebSocketServer } = require("ws");

function hello(intervalMs) {
  return JSON.stringify({ op: 10, d: { heartbeat_interval: intervalMs } });
}

function ready(sessionId, resumeUrl, seq) {
  return JSON.stringify({
    op: 0,
    t: "READY",
    s: seq,
    d: {
      v: 10,
      session_id: sessionId,
      resume_gateway_url: resumeUrl,
      user: { id: "1", username: "bot", discriminator: "0" },
    },
  });
}

function ack() {
  return JSON.stringify({ op: 11, d: null });
}

function startMockGateway(options) {
  const { extraFrames = [], silent = false, intervalMs = 100 } = options || {};
  const seen = { identifies: 0, heartbeats: 0 };
  const server = new WebSocketServer({ port: 0, host: "127.0.0.1" });
  server.on("connection", (ws, req) => {
    const base = `ws://127.0.0.1:${server.address().port}${req.url}`;
    ws.send(hello(intervalMs));
    ws.on("message", (raw) => {
      let frame;
      try {
        frame = JSON.parse(String(raw));
      } catch {
        return;
      }
      if (frame.op === 2) {
        seen.identifies += 1;
        if (silent) {
          return;
        }
        ws.send(ready("sess-1", base, 1));
        for (const extra of extraFrames) {
          ws.send(typeof extra === "string" ? extra : JSON.stringify(extra));
        }
      } else if (frame.op === 1) {
        seen.heartbeats += 1;
        ws.send(ack());
      }
    });
  });
  return new Promise((resolve) => {
    server.on("listening", () => {
      resolve({ server, seen, port: server.address().port });
    });
  });
}

function stop(server) {
  return new Promise((resolve) => {
    for (const client of server.clients) {
      client.terminate();
    }
    server.close(resolve);
  });
}

module.exports = { startMockGateway, stop, hello, ready, ack };
