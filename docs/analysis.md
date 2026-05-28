# Copycara DLP — Production Readiness Analysis

**Date:** 2026-05-28
**Version:** 0.2.1
**Based on:** 3 months of development + real-world usage on `ai-codec` project (3 remotes: origin, local, private; 2 developers; 2 machines)

---

## 1. Executive Summary

Copycara is **functional and usable** for solo developers and small teams who understand the tradeoffs. The core DLP mechanism (tree-sitter AST comment removal) works correctly across 90+ file extensions including CUDA (via `extension_map`). Clean code goes to public remotes; dirty code (with comments) stays in private backup. Data never leaks to public remotes when `copycara push` is used correctly.

**Rating: 3.5/5 — Beta progressing toward Release Candidate**

---

## 2. Achieved (What Works Well)

| Feature | Status | Notes |
|---------|--------|-------|
| Comment scrubbing (tree-sitter AST) | ✅ Verified in production | 90+ extensions including CUDA, Python, C++, YAML, TOML |
| Push to multiple public remotes | ✅ | `copycara push` → origin + local, both clean |
| Private backup with `.copycara/config.toml` | ✅ | Config is trackable in dirty repo, automagically stripped from shadow |
| File deletion sync | ✅ Fixed in v0.2.1 | `git read-tree --reset -u` handles deletions where `git checkout` did not |
| Pre-push hook blocks all public remotes | ✅ | Blocks `git push origin <branch>` AND `git push local <branch>` |
| `.copycara/.ignore` (file exclusion) | ✅ | User-controllable. Default: `/copycara/` hides copycara itself |
| Smart re-init | ✅ | `copycara init` on existing setup: preserves config, updates refspecs, reinstalls hooks |
| Push only current branch (public + private) | ✅ Fixed in v0.2.1 | Old version pushed ALL branches → non-fast-forward on unrelated branches |
| Binary distribution | ✅ | `install.sh` + release workflow (Linux/macOS/Windows via GH Actions) |
| E2E tests (42 assertions) | ✅ | CI runs `cargo test` + `test.sh` sandbox |

**Biggest fixed bug:** The original `git checkout <hash> -- .` in commit processing did NOT handle file deletions. A deleted file like `conv2d_1x1.cu` persisted in shadow commits and leaked to public. Fixed by replacing with `git read-tree --reset -u <hash>`.

---

## 3. Pain Points Discovered in Real Use

### 3.1 First Push After `copycara init` Requires `--force`

**Symptom:**
```
Push rejected — the shadow ref for 'feat/integerization'
has no common ancestor with origin.
Fix: copycara push --force
```

**Root cause:** Shadow ref (`refs/copycara/heads/<branch>`) is created as an orphan commit during init. On a branch that already has history on origin, git rejects non-fast-forward.

**Impact:** Every developer who clones the repo must `copycara init` then `copycara push --force` the first time. The `--force` is safe (`--force-with-lease` is used), but it's a mental barrier.

**Status:** Error is detected and a clear fix message is shown. But the UX barrier remains.

**Mitigation:** After `copycara push --force`, subsequent pushes work normally.

### 3.2 Shadow Refs Not Cloned with Private Repo

**Symptom:** After `git clone private` on a new machine:
- Dirty branches exist (with comments) ✅
- `.copycara/` exists ✅
- **No shadow refs (`refs/copycara/heads/*`)** ❌
- `copycara init` creates new orphan shadow → see 3.1

**Root cause:** Private remote refspec only pushes `refs/heads/*` and `refs/notes/copycara-map`, not `refs/copycara/heads/*`.

**Fix:** Add `refs/copycara/heads/*:refs/copycara/heads/*` to private remote's push refspec. Then shadow refs are backed up and cloned.

**Priority:** P0 (blocks smooth onboarding).

### 3.3 Notes Map Diverges Between Machines

**Symptom:**
```
! [rejected] refs/notes/copycara-map -> refs/notes/copycara-map (fetch first)
```

**Root cause:** If two machines independently process different dirty commits, their local notes maps diverge. Both try to push to the same `refs/notes/copycara-map` on private — git rejects non-fast-forward.

**Status:** We added graceful fallback: if notes are rejected, push dirty branch without notes.

**Long-term fix:** Force-push notes unconditionally (last-writer-wins), or use per-developer notes refs.

### 3.4 Bootstrap Complexity

Steps to set up copycara on a project with existing history:
1. `copycara init`
2. Add `extension_map` for non-standard languages if needed (CUDA: `cu → cpp`)
3. Add `.copycara/.ignore` entries for sensitive files
4. Commit `.copycara/config.toml` to dirty repo (trackable via `!config.toml`)
5. `copycara push --force` (first time, see 3.1)
6. Wrap copycara instructions in `<!-- COPYCARA-BLOCK -->` in AGENTS.md
7. Ensure AI agent knows to use `copycara push` not raw git commands

**Mitigation:** The `copycara init` + `copycara push --force` combo covers 90% of needs. The steps above are for advanced usage.

### 3.5 AGENTS.md via `extension_map = { md = "html" }`

**Symptom:** To stealthily include Copycara instructions in AGENTS.md, we wrap them in HTML comments and use `extension_map = { md = "html" }`. The tree-sitter HTML parser strips `<!-- ... -->` comments from markdown.

**Risks:** Markdown is not valid HTML. Some AGENTS.md files with complex markup may cause the HTML parser to fail or produce unexpected output.

**Mitigation:** If the parser fails, the file is skipped (graceful, not crash). Keep the HTML comment block simple (no nested markdown tables inside the comment).

---

## 4. Missing Functionality (Next Priority)

### 4.1 `copycara status` Command

Show health of copycara in the current repo:

```
$ copycara status
✓  copycara initialized
✓  Shadow ref exists for feat/integerization
✓  Dirty HEAD matches latest shadow commit
✓  2 public remotes configured: origin, local
✓  1 private remote configured: private
✓  Last push: 2 minutes ago
⚠  Notes map diverged from private
```

**Why:** Developer confidence. Currently there's no way to verify the system is healthy without looking at git refs directly.

### 4.2 Shadow Ref Persistence Across Clones

**Needed:** When cloning from private, shadow refs should come along so the new machine doesn't need `--force` bootstrap.

**Implementation in `init.rs`:**
```rust
// In setup_refspecs, for private remotes:
git config --add remote.private.push "refs/copycara/heads/*:refs/copycara/heads/*"
```

This also requires telling users to `git fetch private` or `git clone private --no-checkout` to get shadow refs.

### 4.3 No-Force Bootstrap for Existing Branches

**Idea:** During init's shadow commit creation (step 6), if the remote already has a `refs/heads/<branch>`, fetch the remote tip and diff against it. The shadow commit could use the remote tip's tree as its base rather than being an orphan.

**Implementation:** Download the remote branch, compute the diff, apply to mirror, DLP, create shadow with proper parent.

**Complexity:** High. Git plumbing for computing cross-repo diffs.

### 4.4 `copycara verify` Command

**An idea:** Compare dirty HEAD tree with shadow HEAD tree to verify semantic equivalence (ignoring comments). Fail if they diverged (e.g., due to a botched merge or DLP logic error).

**Implementation:** `git diff-tree --no-renames <dirty>:<path> <shadow>:<path>` for each tracked file. If only comment lines differ → passes. If any code lines differ → fails.

### 4.5 CI Integration Template

**Needed:** Show how to set up copycara in CI:

```yaml
# .gitlab-ci.yml or .github/workflows/ci.yml
before_script:
  - cargo install --git https://github.com/vovasilenko/copycara.git
  - copycara init
```

---

## 5. Recommendations by Priority

| Priority | Feature | Impact |
|----------|---------|--------|
| P0 (next release) | Shadow ref persistence in private backup | Eliminates `--force` bootstrap on new clone |
| P0 (next release) | Force-push notes (last-writer-wins) | Eliminates notes rejection warning |
| P1 | `copycara status` command | Developer confidence, debugging |
| P2 | CI integration template | Faster setup for new projects |
| P2 | `copycara verify` command | Safety net against silent corruption |
| P3 | Native Markdown support in uncomment crate | Cleaner AGENTS.md handling |

---

## 6. Conclusion

Copycara is **ready for real use** by solo developers and small teams. Core thesis proven:

> Comment scrubbing via tree-sitter AST works reliably. File deletions and additions are handled correctly. The two-plane architecture (dirty workspace ↔ clean mirror) is sound. Private remotes store everything; public remotes see nothing sensitive.

The tool has been battle-tested on a real CUDA/Python production project with:
- 3 remotes (origin → public GitLab, local → mirror GitLab, private → GitHub backup)
- 2 machines (original + clone)
- 1 AI agent developing code with heavy annotations (GRACE methodology)
- Real file deletions, renames, merges, amends

**Remaining gaps** (notes sync, shadow persistence, status command) are quality-of-life improvements, not blockers. Each has a documented workaround and a clear path to resolution.

**Ship v0.3.0** with shadow ref persistence in private backup + force-push notes, and it's production-grade for multi-machine teams.
