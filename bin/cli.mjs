#!/usr/bin/env node

import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  mkdirSync,
  existsSync,
  cpSync,
} from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import prompts from "prompts";

const __dirname = dirname(fileURLToPath(import.meta.url));
const TEMPLATES = join(__dirname, "..", "templates");
const ROOT = join(__dirname, "..");

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const targetArg = args.find((a) => !a.startsWith("--"));
const target = targetArg ? resolve(targetArg) : process.cwd();

function readTemplate(rel) {
  return readFileSync(join(TEMPLATES, rel), "utf8");
}

function writeOrAppend(file, content) {
  const dest = join(target, file);
  if (existsSync(dest)) {
    const existing = readFileSync(dest, "utf8");
    if (existing.includes("## Codex Autonomous Workflow")) {
      console.log("  skip (already has harness section): " + file);
      return;
    }
    const separator = "\n\n---\n\n";
    if (!dryRun) {
      appendFileSync(dest, separator + content);
    }
    console.log("  " + (dryRun ? "[dry-run] " : "") + "appended to: " + file);
  } else {
    if (!dryRun) {
      mkdirSync(dirname(dest), { recursive: true });
      writeFileSync(dest, content);
    }
    console.log("  " + (dryRun ? "[dry-run] " : "") + "wrote: " + file);
  }
}

function copyDir(srcRel, destRel) {
  const src = join(TEMPLATES, srcRel);
  const dest = join(target, destRel);
  if (!existsSync(src)) return;
  if (!dryRun) {
    mkdirSync(dirname(dest), { recursive: true });
    cpSync(src, dest, { recursive: true });
  }
  console.log("  " + (dryRun ? "[dry-run] " : "") + "copied: " + destRel + "/");
}

async function main() {
  console.log("\ncodex-github-harness init\n");

  const { mode } = await prompts({
    type: "select",
    name: "mode",
    message: "Which workflow do you want?",
    choices: [
      { title: "Full autonomous (branches, PRs, self-review)", value: "full" },
      { title: "Minimal (local edits, no auto-PR)", value: "minimal" },
    ],
    initial: 0,
  });

  if (!mode) {
    console.log("Aborted.");
    process.exit(0);
  }

  const { skillScope } = await prompts({
    type: "select",
    name: "skillScope",
    message: "Where should skills be installed?",
    choices: [
      { title: "Global (~/.codex/skills/)", value: "global" },
      { title: "Local (./skills/ in this repo)", value: "local" },
    ],
    initial: 0,
  });

  const { branchPrefix } = await prompts({
    type: "text",
    name: "branchPrefix",
    message: "Branch prefix for task branches?",
    initial: "codex/",
  });

  const { worktreeDir } = await prompts({
    type: "text",
    name: "worktreeDir",
    message: "Worktree directory pattern (relative to repo)?",
    initial: "../codex-worktrees/<task>",
  });

  console.log("\nTarget: " + target + "\n");

  // Write or append AGENTS.md
  const agentsFile =
    mode === "full" ? "AGENTS.md" : "examples/AGENTS.minimal.md";
  const agentsContent = readTemplate(agentsFile)
    .replace(/codex\//g, branchPrefix)
    .replace(/\.\.\/codex-worktrees\//g, worktreeDir.replace(/<task>/g, ""));
  writeOrAppend("AGENTS.md", agentsContent);

  // Install skills
  if (skillScope === "global") {
    const skillsDest = join(
      process.env.HOME || process.env.USERPROFILE,
      ".codex",
      "skills",
    );
    if (!dryRun) {
      mkdirSync(skillsDest, { recursive: true });
      cpSync(
        join(TEMPLATES, "skills", "github-pr-workflow"),
        join(skillsDest, "github-pr-workflow"),
        { recursive: true },
      );
      cpSync(
        join(TEMPLATES, "skills", "post-implementation-review"),
        join(skillsDest, "post-implementation-review"),
        { recursive: true },
      );
    }
    console.log(
      "  " +
        (dryRun ? "[dry-run] " : "") +
        "installed skills to ~/.codex/skills/",
    );
  } else {
    copyDir("skills", "skills");
  }

  // Copy docs and examples into repo
  copyDir("docs", "docs");
  copyDir("examples", "examples");

  // Write LICENSE
  const license = readFileSync(join(ROOT, "LICENSE"), "utf8");
  const licenseDest = join(target, "LICENSE");
  if (!existsSync(licenseDest)) {
    if (!dryRun) {
      writeFileSync(licenseDest, license);
    }
    console.log("  " + (dryRun ? "[dry-run] " : "") + "wrote: LICENSE");
  } else {
    console.log("  skip (exists): LICENSE");
  }

  console.log("\nDone.");
  console.log("\nNext steps:");
  console.log("  1. Review AGENTS.md in your repo root.");
  if (mode === "full") {
    console.log("  2. Make sure gh is authenticated: gh auth status");
    console.log("  3. Start Codex and ask it to implement a task.");
  } else {
    console.log("  2. Start Codex and ask it to implement a task.");
  }
  console.log();
}

main().catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
