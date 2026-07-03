import { describe, it } from "node:test";
import { strict as assert } from "node:assert";
import { existsSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const TEMPLATES = join(ROOT, "templates");

describe("templates", () => {
  it("has AGENTS.md", () => {
    assert.ok(existsSync(join(TEMPLATES, "AGENTS.md")));
  });

  it("has minimal example", () => {
    assert.ok(existsSync(join(TEMPLATES, "examples", "AGENTS.minimal.md")));
  });

  it("has full example", () => {
    assert.ok(existsSync(join(TEMPLATES, "examples", "AGENTS.full.md")));
  });

  it("has github-pr-workflow skill", () => {
    assert.ok(
      existsSync(join(TEMPLATES, "skills", "github-pr-workflow", "SKILL.md")),
    );
  });

  it("has post-implementation-review skill", () => {
    assert.ok(
      existsSync(
        join(TEMPLATES, "skills", "post-implementation-review", "SKILL.md"),
      ),
    );
  });

  it("has docs", () => {
    assert.ok(existsSync(join(TEMPLATES, "docs", "workflow.md")));
    assert.ok(existsSync(join(TEMPLATES, "docs", "customization.md")));
    assert.ok(existsSync(join(TEMPLATES, "docs", "faq.md")));
  });
});

describe("package.json", () => {
  it("has correct name and bin", () => {
    const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
    assert.equal(pkg.name, "codex-github-harness");
    assert.ok(pkg.bin["codex-github-harness"]);
  });
});
