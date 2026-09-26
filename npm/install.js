#!/usr/bin/env node
"use strict";
// postinstall：按当前平台从 GitHub Release 下载对应的预编译二进制。
// 不编译（纯二进制分发），下载失败不阻断 npm install（仅告警）。
const path = require("path");
const fs = require("fs");
const crypto = require("crypto");
const https = require("https");
const { execFileSync } = require("child_process");

const pkg = require("./package.json");
const TAG = "v" + pkg.version;
const REPO = "Gollum-code/prompt-git";
const BASE = "https://github.com/" + REPO + "/releases/download/" + TAG;

// 平台 -> Release 资产（与 .github/workflows/release.yml 对应）
const TARGETS = {
  "win32-x64": { target: "x86_64-pc-windows-msvc", ext: ".zip" },
  "darwin-arm64": { target: "aarch64-apple-darwin", ext: ".tar.gz" },
  "linux-x64": { target: "x86_64-unknown-linux-gnu", ext: ".tar.gz" },
};

function download(url, redirects) {
  redirects = redirects || 0;
  return new Promise((resolve, reject) => {
    https
      .get(url, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          res.resume();
          if (redirects > 5) return reject(new Error("too many redirects"));
          return resolve(download(res.headers.location, redirects + 1));
        }
        if (res.statusCode !== 200) {
          res.resume();
          return reject(new Error("HTTP " + res.statusCode));
        }
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => resolve(Buffer.concat(chunks)));
      })
      .on("error", reject);
  });
}

async function fetchWithRetry(url, tries) {
  tries = tries || 3;
  let lastErr;
  for (let i = 0; i < tries; i++) {
    try {
      return await download(url);
    } catch (e) {
      lastErr = e;
      await new Promise((r) => setTimeout(r, 1000 * (i + 1)));
    }
  }
  throw lastErr;
}

function sha256(buf) {
  return crypto.createHash("sha256").update(buf).digest("hex");
}

async function main() {
  const key = process.platform + "-" + process.arch;
  const info = TARGETS[key];
  if (!info) {
    console.warn(
      "[prompt-git] 暂不支持平台 " + key + "，跳过二进制下载。"
    );
    console.warn("  手动下载: https://github.com/" + REPO + "/releases");
    return;
  }

  const asset = "prompt-git-" + info.target + info.ext;
  const outDir = path.join(__dirname, "binaries");
  fs.mkdirSync(outDir, { recursive: true });

  console.log("[prompt-git] 下载 " + asset + " (" + TAG + ") ...");
  const data = await fetchWithRetry(BASE + "/" + asset);

  // sha256 校验（与 release 附带的 checksums 一致）
  try {
    const sums = (
      await fetchWithRetry(BASE + "/checksums-" + info.target + ".txt")
    ).toString("utf8");
    const line = sums
      .split(/\r?\n/)
      .find((l) => l.indexOf(asset) !== -1);
    if (line) {
      const expected = line.trim().split(/\s+/)[0];
      const actual = sha256(data);
      if (expected !== actual) {
        throw new Error("sha256 不匹配 expected=" + expected + " got=" + actual);
      }
      console.log("[prompt-git] sha256 校验通过");
    }
  } catch (e) {
    console.warn("[prompt-git] sha256 校验跳过: " + e.message);
  }

  // 解压（Windows 10+ 自带 bsdtar，同时支持 .zip 与 .tar.gz）
  const tmp = path.join(outDir, "download" + info.ext);
  fs.writeFileSync(tmp, data);
  try {
    execFileSync("tar", ["-xf", tmp, "-C", outDir], { stdio: "inherit" });
  } finally {
    fs.rmSync(tmp, { force: true });
  }

  const exe = process.platform === "win32" ? "prompt-git.exe" : "prompt-git";
  const binPath = path.join(outDir, exe);
  if (!fs.existsSync(binPath)) throw new Error("解压后未找到 " + exe);
  if (process.platform !== "win32") fs.chmodSync(binPath, 0o755);
  console.log("[prompt-git] v" + pkg.version + " 安装完成 (" + key + ")");
}

main().catch((err) => {
  // 不阻断 npm install，仅告警并给出手动下载入口
  console.warn("[prompt-git] 二进制下载失败: " + err.message);
  console.warn("  手动下载: https://github.com/" + REPO + "/releases");
  process.exit(0);
});
