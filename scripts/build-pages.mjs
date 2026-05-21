#!/usr/bin/env node
import { mkdirSync, writeFileSync, copyFileSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = resolve(root, "pages-dist");
mkdirSync(outDir, { recursive: true });

const repo = process.env.GH_REPO || "UrubuCode/OpenPtl";
const token = process.env.GH_TOKEN;

async function gh(path) {
  const res = await fetch(`https://api.github.com${path}`, {
    headers: {
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      "User-Agent": "openptl-pages-builder",
    },
  });
  if (!res.ok) throw new Error(`GitHub ${path} -> ${res.status}`);
  return res.json();
}

let releases = [];
try {
  releases = await gh(`/repos/${repo}/releases?per_page=50`);
} catch (err) {
  console.warn(`Could not fetch releases: ${err.message}`);
}

function classify(name) {
  const n = name.toLowerCase();
  if (n.endsWith(".msi") || n.endsWith(".exe")) return "windows";
  if (n.endsWith(".dmg") || n.endsWith(".app.tar.gz")) return "macos";
  if (n.endsWith(".appimage") || n.endsWith(".deb") || n.endsWith(".rpm")) return "linux";
  return "other";
}

const icons = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
  other: "Other",
};

const releasesHtml = releases
  .filter((r) => !r.draft)
  .map((r) => {
    const grouped = { windows: [], macos: [], linux: [], other: [] };
    for (const a of r.assets) grouped[classify(a.name)].push(a);
    const groupsHtml = Object.entries(grouped)
      .filter(([, list]) => list.length)
      .map(
        ([os, list]) => `
          <div class="os-group">
            <h4>${icons[os]}</h4>
            <ul>
              ${list
                .map(
                  (a) =>
                    `<li><a href="${a.browser_download_url}">${a.name}</a> <span class="size">${(a.size / 1024 / 1024).toFixed(1)} MB</span></li>`,
                )
                .join("")}
            </ul>
          </div>`,
      )
      .join("");
    return `
      <article class="release">
        <header>
          <h3>${r.name || r.tag_name} ${r.prerelease ? '<span class="badge">pre-release</span>' : ""}</h3>
          <time datetime="${r.published_at}">${new Date(r.published_at).toISOString().slice(0, 10)}</time>
        </header>
        <div class="downloads">${groupsHtml || "<p>No downloadable assets.</p>"}</div>
        ${r.body ? `<details><summary>Release notes</summary><pre>${escapeHtml(r.body)}</pre></details>` : ""}
      </article>`;
  })
  .join("");

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}

const latest = releases.find((r) => !r.draft && !r.prerelease) || releases[0];
const latestTag = latest?.tag_name ?? "—";

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<title>OpenPtl — Downloads</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body { font: 15px/1.5 system-ui, sans-serif; margin: 0; background: #0b0d11; color: #e6e8eb; }
  header.hero { padding: 64px 24px; text-align: center; background: linear-gradient(180deg, #14171c, #0b0d11); border-bottom: 1px solid #1f242c; }
  header.hero h1 { font-size: 42px; margin: 0 0 8px; letter-spacing: -0.5px; }
  header.hero p { color: #9aa3ad; margin: 0; }
  .latest-tag { display: inline-block; margin-top: 14px; padding: 4px 12px; border: 1px solid #2a3038; border-radius: 999px; font-size: 13px; color: #c6cdd5; }
  main { max-width: 880px; margin: 0 auto; padding: 32px 24px 80px; }
  h2 { font-size: 22px; margin: 32px 0 16px; }
  .release { background: #12161c; border: 1px solid #1f242c; border-radius: 12px; padding: 20px 22px; margin-bottom: 18px; }
  .release header { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; margin-bottom: 12px; }
  .release h3 { margin: 0; font-size: 18px; }
  .release time { color: #7f8893; font-size: 13px; }
  .badge { display: inline-block; margin-left: 8px; padding: 1px 8px; font-size: 11px; background: #3a2c10; color: #f3c270; border-radius: 4px; vertical-align: middle; }
  .downloads { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
  .os-group { background: #0d1015; border: 1px solid #1a1f26; border-radius: 8px; padding: 12px 14px; }
  .os-group h4 { margin: 0 0 8px; font-size: 13px; color: #9aa3ad; text-transform: uppercase; letter-spacing: 0.5px; }
  .os-group ul { list-style: none; padding: 0; margin: 0; }
  .os-group li { margin-bottom: 6px; word-break: break-all; }
  .os-group a { color: #7eb3ff; text-decoration: none; }
  .os-group a:hover { text-decoration: underline; }
  .size { color: #6b7480; font-size: 12px; }
  details { margin-top: 12px; }
  details summary { cursor: pointer; color: #9aa3ad; font-size: 13px; }
  details pre { white-space: pre-wrap; background: #0d1015; padding: 12px; border-radius: 6px; font-size: 13px; color: #c6cdd5; overflow-x: auto; }
  footer { text-align: center; color: #6b7480; font-size: 13px; padding: 24px; }
  footer a { color: #7eb3ff; }
</style>
</head>
<body>
  <header class="hero">
    <h1>OpenPtl</h1>
    <p>Multi-protocol remote workspace — SSH · SFTP · FTP · SMB · RDP</p>
    <div class="latest-tag">Latest: <strong>${latestTag}</strong></div>
  </header>
  <main>
    <h2>Releases</h2>
    ${releasesHtml || "<p>No releases yet.</p>"}
  </main>
  <footer>
    <a href="https://github.com/${repo}">github.com/${repo}</a>
  </footer>
</body>
</html>`;

writeFileSync(resolve(outDir, "index.html"), html);

const nojekyll = resolve(outDir, ".nojekyll");
writeFileSync(nojekyll, "");

console.log(`Built pages-dist with ${releases.length} release(s).`);
