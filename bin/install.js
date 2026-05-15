#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");
const os = require("os");
const cp = require("child_process");

const SKILL_NAME = "codex-usage-dashboard";
const repoRoot = path.resolve(__dirname, "..");
const sourceSkill = path.join(repoRoot, "skill", SKILL_NAME);
const codexHome = process.env.CODEX_HOME || path.join(os.homedir(), ".codex");
const targetRoot = path.join(codexHome, "skills");
const targetSkill = path.join(targetRoot, SKILL_NAME);
const args = new Set(process.argv.slice(2));

function copyRecursive(src, dest) {
  const stat = fs.statSync(src);
  if (stat.isDirectory()) {
    fs.mkdirSync(dest, { recursive: true });
    for (const entry of fs.readdirSync(src)) {
      if (entry === "__pycache__" || entry === ".DS_Store") continue;
      copyRecursive(path.join(src, entry), path.join(dest, entry));
    }
    return;
  }
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(src, dest);
}

function removeIfExists(target) {
  if (fs.existsSync(target)) {
    fs.rmSync(target, { recursive: true, force: true });
  }
}

function findPython() {
  const candidates = process.platform === "win32"
    ? ["python.exe", "py.exe"]
    : ["python3", "python"];

  for (const candidate of candidates) {
    const result = cp.spawnSync(candidate, ["--version"], { stdio: "ignore" });
    if (result.status === 0) return candidate;
  }
  return null;
}

function runPython(script) {
  const python = findPython();
  if (!python) {
    console.warn("Python was not found. The skill is installed, but shortcut/open commands need Python 3.");
    return;
  }
  cp.spawnSync(python, [script], { stdio: "inherit" });
}

if (!fs.existsSync(sourceSkill)) {
  console.error(`Missing bundled skill: ${sourceSkill}`);
  process.exit(1);
}

fs.mkdirSync(targetRoot, { recursive: true });
if (args.has("--force") || args.has("-f")) {
  removeIfExists(targetSkill);
}
copyRecursive(sourceSkill, targetSkill);

console.log(`Installed ${SKILL_NAME} to: ${targetSkill}`);
console.log("");
console.log("Codex-only: this is a Codex skill and currently only works in OpenAI Codex.");
console.log("Restart Codex or open a new Codex conversation, then ask:");
console.log("  Use $codex-usage-dashboard to open my Codex usage dashboard");
console.log("");
console.log("Direct launch:");
console.log(`  python "${path.join(targetSkill, "scripts", "open_dashboard.py")}"`);

if (args.has("--shortcut")) {
  runPython(path.join(targetSkill, "scripts", "install_desktop_shortcut.py"));
}
if (args.has("--open")) {
  runPython(path.join(targetSkill, "scripts", "open_dashboard.py"));
}
