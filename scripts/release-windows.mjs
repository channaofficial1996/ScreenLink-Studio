#!/usr/bin/env node
// Builds the Windows installers, signs them with the local update key, and publishes a GitHub
// Release with the latest.json that installed copies check. See docs/RELEASING.md.
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "channaofficial1996/ScreenLink-Studio";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const { version } = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
const tag = `v${version}`;
const keyFile = join(homedir(), ".tauri", "screenlink-studio.key");
if (!existsSync(keyFile)) throw new Error(`Signing key not found at ${keyFile}`);

const run = (cmd, args, env = {}) =>
  execFileSync(cmd, args, { cwd: root, stdio: "inherit", shell: cmd === "npx", env: { ...process.env, ...env } });

if (!process.argv.includes("--skip-build")) {
  run("npx", ["tauri", "build"], {
    TAURI_SIGNING_PRIVATE_KEY: readFileSync(keyFile, "utf8"),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "",
  });
}

const bundle = join(root, "src-tauri/target/release/bundle");
const setup = join(bundle, "nsis", `ScreenLink Studio_${version}_x64-setup.exe`);
const msi = join(bundle, "msi", `ScreenLink Studio_${version}_x64_en-US.msi`);
const signature = readFileSync(`${setup}.sig`, "utf8").trim();

// GitHub replaces spaces in asset names with dots.
const assetName = `ScreenLink.Studio_${version}_x64-setup.exe`;
const url = `https://github.com/${REPO}/releases/download/${tag}/${assetName}`;
const manifest = {
  version,
  notes: `ScreenLink Studio ${tag}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64-nsis": { signature, url },
    "windows-x86_64": { signature, url },
  },
};
const latest = join(mkdtempSync(join(tmpdir(), "slstudio-release-")), "latest.json");
writeFileSync(latest, JSON.stringify(manifest, null, 2));

run("gh", [
  "release", "create", tag,
  "-R", REPO,
  "--title", `ScreenLink Studio ${tag}`,
  "--notes", "Download `ScreenLink.Studio_" + version + "_x64-setup.exe` and run it. Windows may show a SmartScreen warning because the app is not code signed yet: choose More info, then Run anyway. Installed copies update themselves.",
  setup, msi, latest,
]);
console.log(`Published ${tag}`);
