#!/usr/bin/env node
// Downloads the two third-party runtime components ScreenLink Studio bundles:
//   - scrcpy-server (Apache-2.0), pinned and verified against the official SHA256SUMS.txt
//   - Android platform-tools (adb), from Google's official repository URL
// Files land in src-tauri/resources/<platform>/ and are picked up by tauri.conf.json.
//
// Usage: node scripts/fetch-sidecars.mjs [--platform=win|mac] [--force]
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, copyFileSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// Must equal SCRCPY_VERSION in src-tauri/src/android/server.rs: the server refuses other client versions.
const SCRCPY_VERSION = "5.0.1";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const force = args.includes("--force");
const platformArg = args.find((a) => a.startsWith("--platform="))?.split("=")[1];
const platform = platformArg ?? (process.platform === "win32" ? "win" : "mac");
if (!["win", "mac"].includes(platform)) throw new Error(`Unsupported platform: ${platform}`);

const resources = join(root, "src-tauri", "resources");
const platformDir = join(resources, "platform-tools");
mkdirSync(platformDir, { recursive: true });

async function download(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`GET ${url} failed: ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

async function fetchServer() {
  const target = join(resources, "scrcpy-server");
  const base = `https://github.com/Genymobile/scrcpy/releases/download/v${SCRCPY_VERSION}`;
  const name = `scrcpy-server-v${SCRCPY_VERSION}`;
  const sums = (await download(`${base}/SHA256SUMS.txt`)).toString("utf8");
  const expected = sums.split("\n").find((l) => l.trim().endsWith(name))?.trim().split(/\s+/)[0];
  if (!expected) throw new Error(`${name} is not listed in SHA256SUMS.txt`);

  if (!force && existsSync(target) && sha256(readFileSync(target)) === expected) {
    console.log("scrcpy-server already present and verified");
    return;
  }
  const data = await download(`${base}/${name}`);
  const actual = sha256(data);
  if (actual !== expected) throw new Error(`scrcpy-server checksum mismatch: ${actual} != ${expected}`);
  writeFileSync(target, data);
  console.log(`scrcpy-server v${SCRCPY_VERSION} verified (sha256 ${actual.slice(0, 16)}…)`);
}

function sha256(buf) {
  return createHash("sha256").update(buf).digest("hex");
}

async function fetchAdb() {
  const adbName = platform === "win" ? "adb.exe" : "adb";
  const files = platform === "win" ? ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll"] : ["adb"];
  if (!force && files.every((f) => existsSync(join(platformDir, f)))) {
    console.log("platform-tools already present");
    return;
  }
  const os = platform === "win" ? "windows" : "darwin";
  const data = await download(`https://dl.google.com/android/repository/platform-tools-latest-${os}.zip`);
  const tmp = mkdtempSync(join(tmpdir(), "slstudio-pt-"));
  try {
    const zip = join(tmp, "pt.zip");
    writeFileSync(zip, data);
    // bsdtar ships with Windows 10+ and macOS and reads zip archives. Use the absolute Windows
    // path because Git Bash puts its GNU tar, which cannot read zips or C: paths, first on PATH.
    const tar = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\Windows", "System32", "tar.exe") : "tar";
    execFileSync(tar, ["-xf", zip, "-C", tmp], { stdio: "inherit" });
    for (const f of [...files, "NOTICE.txt"]) {
      const dest = join(platformDir, f === "NOTICE.txt" ? "platform-tools-NOTICE.txt" : f);
      copyFileSync(join(tmp, "platform-tools", f), dest);
    }
    if (platform !== "win") chmodSync(join(platformDir, adbName), 0o755);
    console.log(`platform-tools for ${os} installed`);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

await fetchServer();
await fetchAdb();
