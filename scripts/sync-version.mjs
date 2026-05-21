#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriConfPath = resolve(root, "src-tauri/tauri.conf.json");
const pkgPath = resolve(root, "package.json");
const cargoPath = resolve(root, "src-tauri/Cargo.toml");

const override = process.argv[2];

const tauriConf = JSON.parse(readFileSync(tauriConfPath, "utf8"));
if (override) {
  tauriConf.version = override;
  writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + "\n");
}
const version = tauriConf.version;
if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version)) {
  console.error(`Invalid version in tauri.conf.json: ${version}`);
  process.exit(1);
}

const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
pkg.version = version;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

let cargo = readFileSync(cargoPath, "utf8");
cargo = cargo.replace(/^version = "[^"]+"/m, `version = "${version}"`);
writeFileSync(cargoPath, cargo);

console.log(`Synced version ${version} -> package.json, Cargo.toml`);
