#!/usr/bin/env node

import { spawn } from "node:child_process";
import os from "node:os";

const tauriArgs = process.argv.slice(2);

if (tauriArgs.length === 0) {
  console.error("Usage: node scripts/run-with-target-dir.mjs <tauri-args...>");
  process.exit(1);
}

const defaultTargetDir = process.platform === "win32"
  ? `${os.tmpdir()}\\dvu_u-target`
  : "/tmp/dvu_u-target";

const targetDir = process.env.DVU_TARGET_DIR || defaultTargetDir;
const env = {
  ...process.env,
  DVU_TARGET_DIR: targetDir,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR || targetDir
};

const child = spawn("npm", ["exec", "--", "tauri", ...tauriArgs], {
  env,
  stdio: "inherit",
  shell: process.platform === "win32"
});

child.on("error", (error) => {
  console.error(error.message);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 1);
});
