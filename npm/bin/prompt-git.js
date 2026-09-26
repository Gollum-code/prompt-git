#!/usr/bin/env node
"use strict";
const path = require("path");
const fs = require("fs");
const { spawnSync } = require("child_process");

const exe = process.platform === "win32" ? "prompt-git.exe" : "prompt-git";
const binPath = path.join(__dirname, "..", "binaries", exe);

if (!fs.existsSync(binPath)) {
  console.error("prompt-git 二进制未找到。请尝试：npm rebuild prompt-git");
  console.error("或从 https://github.com/Gollum-code/prompt-git/releases 手动下载。");
  process.exit(1);
}

const r = spawnSync(binPath, process.argv.slice(2), { stdio: "inherit" });
if (r.error) {
  console.error("启动 prompt-git 失败: " + r.error.message);
  process.exit(1);
}
process.exit(r.status === null ? 1 : r.status);