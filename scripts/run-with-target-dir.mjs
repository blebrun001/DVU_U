#!/usr/bin/env node

import { spawn } from "node:child_process";
import os from "node:os";

const args = process.argv.slice(2);

if (args.length === 0) {
  console.error("Usage: node scripts/run-with-target-dir.mjs <command> [args...]");
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

const [command, ...commandArgs] = args;
const child = spawn(command, commandArgs, {
  env,
  stdio: "inherit"
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
