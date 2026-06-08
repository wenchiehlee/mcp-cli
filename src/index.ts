#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

const releaseBinary = join(import.meta.dir, "..", "target", "release", "mcp-cli");
const debugBinary = join(import.meta.dir, "..", "target", "debug", "mcp-cli");

const rustBinary = existsSync(releaseBinary) ? releaseBinary : debugBinary;

const result = spawnSync(rustBinary, process.argv.slice(2), {
  stdio: "inherit",
  env: process.env,
});

process.exit(result.status ?? 1);
