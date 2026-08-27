// Serve the showcase page with the Agentation toolbar overlaid.
//
//   node serve.mjs [showcase-dir] [port]
//
// `showcase-dir` is whatever `sentinel export --ui <dir>` wrote — it supplies
// `bundle.json`. The HTML comes from `../index.html` in the tree, not from that
// directory, so editing the real page and reloading is the whole loop. The
// toolbar script is injected into the response and never written to disk.

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const here = fileURLToPath(new URL(".", import.meta.url));
const pageFile = resolve(here, "..", "index.html");
const showcase = resolve(process.argv[2] ?? "/tmp/clone-showcase");
const port = Number(process.argv[3] ?? 8123);

const built = await esbuild.build({
  entryPoints: [resolve(here, "overlay.jsx")],
  bundle: true,
  format: "iife",
  write: false,
  minify: true,
  define: { "process.env.NODE_ENV": '"development"' },
  loader: { ".jsx": "jsx" },
});
const overlay = built.outputFiles[0].text;

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
};

createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, "http://x").pathname);

  if (path === "/__agentation.js") {
    res.writeHead(200, { "content-type": TYPES[".js"] });
    return res.end(overlay);
  }

  if (path === "/" || path === "/index.html") {
    // Read on every request, so a save-and-reload shows the edit. Caching the
    // page here would make the dev server the one place the loop does not work.
    const html = await readFile(pageFile, "utf8");
    res.writeHead(200, {
      "content-type": TYPES[".html"],
      "cache-control": "no-store",
    });
    return res.end(`${html}\n<script src="/__agentation.js"></script>\n`);
  }

  // Everything else out of the exported showcase: bundle.json, and whatever a
  // later version of the page decides to ask for.
  try {
    const file = join(showcase, path);
    if (!file.startsWith(showcase)) {
      res.writeHead(403).end("outside the showcase directory");
      return;
    }
    const body = await readFile(file);
    res.writeHead(200, {
      "content-type": TYPES[extname(file)] ?? "application/octet-stream",
      "cache-control": "no-store",
    });
    res.end(body);
  } catch {
    res.writeHead(404, { "content-type": "text/plain" });
    res.end(`not found: ${path}\n(serving data from ${showcase})`);
  }
}).listen(port, () => {
  console.log(`  page      ${pageFile}`);
  console.log(`  data      ${showcase}`);
  console.log(`  toolbar   overlaid at request time, never written to the page`);
  console.log(`\n  http://localhost:${port}\n`);
});
