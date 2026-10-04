// A deliberately plain static file server for end-to-end tests. Serving the
// production bundle with nothing but file reads proves there is no hidden
// server dependency (DESIGN §26.3, criterion 18).
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize, resolve } from "node:path";

// usage: node static-server.mjs <root> <port> [base path, e.g. /nested/app/]
const root = resolve(process.argv[2] ?? "dist");
const port = Number(process.argv[3] ?? 4173);
const base = process.argv[4] ?? "/";

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".json": "application/json",
};

createServer((request, response) => {
  // Only GET is meaningful for static files; anything else would mean the app expects a backend.
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.writeHead(405).end();
    return;
  }
  const urlPath = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
  if (!urlPath.startsWith(base)) {
    response.writeHead(404).end("not found");
    return;
  }
  let file = normalize(join(root, urlPath.slice(base.length)));
  if (!file.startsWith(root)) {
    response.writeHead(403).end();
    return;
  }
  if (existsSync(file) && statSync(file).isDirectory()) file = join(file, "index.html");
  if (!existsSync(file)) {
    response.writeHead(404).end("not found");
    return;
  }
  response.writeHead(200, { "content-type": MIME[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(response);
}).listen(port, () => console.log(`static: ${root} on http://localhost:${port}${base}`));
