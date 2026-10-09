// Builds the `trax-mcp` MCP server and places it where Tauri's bundler
// expects external binaries: src-tauri/binaries/trax-mcp-<target-triple>[.exe].
// The installers then ship it next to the app, so AI tools can launch it.
//
// Usage: node scripts/build-sidecar.mjs [--debug] [--target <triple>]

import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const debug = args.includes("--debug");
const targetArg = args.includes("--target") ? args[args.indexOf("--target") + 1] : undefined;

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const line = out.split("\n").find((l) => l.startsWith("host:"));
  if (!line) throw new Error("could not determine the Rust host triple");
  return line.slice("host:".length).trim();
}

// Tauri exports the triple it is building for to before*Command hooks.
const target = targetArg || process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple();
const explicitTarget = Boolean(targetArg || process.env.TAURI_ENV_TARGET_TRIPLE);
const profile = debug ? "debug" : "release";

const cargoArgs = ["build", "-p", "trax-mcp", "--bin", "trax-mcp"];
if (!debug) cargoArgs.push("--release");
if (explicitTarget) cargoArgs.push("--target", target);

console.log(`[sidecar] cargo ${cargoArgs.join(" ")}`);
execFileSync("cargo", cargoArgs, { cwd: root, stdio: "inherit" });

const exe = target.includes("windows") ? ".exe" : "";
const built = explicitTarget
  ? join(root, "target", target, profile, `trax-mcp${exe}`)
  : join(root, "target", profile, `trax-mcp${exe}`);
const destDir = join(root, "src-tauri", "binaries");
const dest = join(destDir, `trax-mcp-${target}${exe}`);
mkdirSync(destDir, { recursive: true });
copyFileSync(built, dest);
if (!exe) chmodSync(dest, 0o755);
console.log(`[sidecar] ${dest}`);
