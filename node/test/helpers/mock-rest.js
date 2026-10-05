// Scripted mock REST server for M4 boundary tests (127.0.0.1 only).
// Serves queued raw bodies per path prefix; records every request.
// Mock secrets only; never production.
const http = require("node:http");

function json(status, obj) {
  const body = JSON.stringify(obj);
  return { status, headers: { "Content-Type": "application/json" }, body };
}

function startMockRest(routes) {
  const observed = [];
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      observed.push({ method: req.method, url: req.url, body });
      const route = routes.find((r) => req.url.startsWith(r.prefix));
      const answer = route ? route.next() : json(200, {});
      res.writeHead(answer.status, answer.headers);
      res.end(answer.body);
    });
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      resolve({ server, observed, port: server.address().port });
    });
  });
}

function stop(server) {
  return new Promise((resolve) => server.close(resolve));
}

module.exports = { json, startMockRest, stop };
