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

async function askQuestions() {
  const answers = {};

  // 1. Workflow mode
  const { mode } = await prompts({
    type: "select",
    name: "mode",
    message: "Which workflow do you want?",
    choices: [
      {
        title: "Full autonomous (branches, worktrees, PRs, self-review)",
        value: "full",
      },
      {
        title: "Minimal (local edits, no auto-PR, commit only when asked)",
        value: "minimal",
      },
    ],
    initial: 0,
  });
  if (!mode) {
    console.log("Aborted.");
    process.exit(0);
  }
  answers.mode = mode;

  // 2. Skill scope
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
  answers.skillScope = skillScope;

  // 3. Branch prefix
  const { branchChoice } = await prompts({
    type: "select",
    name: "branchChoice",
    message: "Branch prefix for task branches?",
    choices: [
      { title: "codex/ (default)", value: "codex/" },
      { title: "Custom prefix", value: "custom" },
    ],
    initial: 0,
  });
  answers.branchChoice = branchChoice;

  if (branchChoice === "custom") {
    const { customPrefix } = await prompts({
      type: "text",
      name: "customPrefix",
      message: "Enter your branch prefix:",
      initial: "codex/",
    });
    answers.branchPrefix = customPrefix || "codex/";
  } else {
    answers.branchPrefix = "codex/";
  }

  // 4. Worktree directory
  const { worktreeChoice } = await prompts({
    type: "select",
    name: "worktreeChoice",
    message: "Where should worktrees be created?",
    choices: [
      {
        title: "../worktrees/<task> (default, outside repo)",
        value: "default",
      },
      { title: "./worktrees/<task> (inside repo)", value: "inside" },
      { title: "Custom path", value: "custom" },
    ],
    initial: 0,
  });
  answers.worktreeChoice = worktreeChoice;

  if (worktreeChoice === "inside") {
    answers.worktreeDir = "./worktrees/";
  } else if (worktreeChoice === "custom") {
    const { customPath } = await prompts({
      type: "text",
      name: "customPath",
      message: "Enter worktree path (relative to repo):",
      initial: "../worktrees/",
    });
    answers.worktreeDir = customPath || "../worktrees/";
  } else {
    answers.worktreeDir = "../worktrees/";
  }

  return answers;
}

function summarize(a) {
  const modeLabel =
    a.mode === "full" ? "Full autonomous" : "Minimal (local only)";
  const skillLabel =
    a.skillScope === "global"
      ? "Global (~/.codex/skills/)"
      : "Local (./skills/)";
  return [
    "  Workflow:    " + modeLabel,
    "  Skills:      " + skillLabel,
    "  Branch:      " + a.branchPrefix,
    "  Worktrees:   " + a.worktreeDir,
  ].join("\n");
}

async function main() {
  console.log("\ncodex-github-harness init\n");

  let answers = await askQuestions();

  // Confirm or redo
  while (true) {
    console.log("\nYour configuration:");
    console.log(summarize(answers));

    const { confirm } = await prompts({
      type: "select",
      name: "confirm",
      message: "Looks good?",
      choices: [
        { title: "Yes, proceed", value: "yes" },
        { title: "No, redo setup", value: "redo" },
      ],
      initial: 0,
    });

    if (confirm === "yes") break;
    if (!confirm) {
      console.log("Aborted.");
      process.exit(0);
    }
    answers = await askQuestions();
  }

  console.log("\nTarget: " + target + "\n");

  // Write or append AGENTS.md
  const agentsFile =
    answers.mode === "full" ? "AGENTS.md" : "examples/AGENTS.minimal.md";
  const agentsContent = readTemplate(agentsFile)
    .replace(/codex\//g, answers.branchPrefix)
    .replace(/\.\.\/worktrees\//g, answers.worktreeDir);
  writeOrAppend("AGENTS.md", agentsContent);

  // Install skills
  if (answers.skillScope === "global") {
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

  // Skills guide
  console.log("\nInstalled skills:");
  console.log(
    "  github-pr-workflow          Branch, worktree, commit, push, and PR flow for GitHub tasks.",
  );
  console.log(
    "  post-implementation-review  Self-review loop: diff check, cleanup, re-verify before reporting done.",
  );

  console.log("\nDone.");
  console.log("\nNext steps:");
  console.log("  1. Review AGENTS.md in your repo root.");
  if (answers.mode === "full") {
    console.log("  2. Make sure gh is authenticated: gh auth status");
    console.log("  3. Start Codex and ask it to implement a task.");
  } else {
    console.log("  2. Start Codex and ask it to implement a task.");
    console.log("  3. Codex will edit locally and commit only when you ask.");
  }
  console.log();
}

main().catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
