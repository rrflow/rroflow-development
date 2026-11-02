// console/src is authored. Rust embeds only these generated, self-contained assets.
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { gzipSync } from "node:zlib";
import assert from "node:assert/strict";
const root = new URL("../", import.meta.url),
  out = new URL("crates/rroflow-ingest/src/member-console/", root);
const manifest = JSON.parse(
  readFileSync(new URL("console/dist/.vite/manifest.json", root), "utf8"),
);
const entry = manifest["index.html"];
assert.equal(entry.file, "app.js");
assert.deepEqual(entry.css, ["index.css"]);
const files = ["index.html", "app.js", "index.css"];
assert.deepEqual(
  readdirSync(new URL("console/dist/", root))
    .filter((x) => x !== ".vite")
    .sort(),
  files.toSorted(),
);
if (!process.argv.includes("--check")) mkdirSync(out, { recursive: true });
for (const file of files) {
  const bytes = readFileSync(new URL("console/dist/" + file, root));
  assert.ok(
    gzipSync(bytes).length <
      { "app.js": 90000, "index.css": 10000, "index.html": 2048 }[file],
    `${file} exceeds initial loading budget`,
  );
  const destination = new URL(file, out);
  if (process.argv.includes("--check"))
    assert.deepEqual(
      bytes,
      readFileSync(destination),
      `Stale ${file}; run console npm run export`,
    );
  else writeFileSync(destination, bytes);
}
assert.deepEqual(
  readdirSync(out).sort(),
  files.toSorted(),
  "orphaned embedded member assets",
);
console.log("Member console embedded assets are current.");
