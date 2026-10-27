# GitCthulhu

> **Index, not data dump.** Point Claude at the right files; don't paraphrase them here.
> Auto-loaded every turn — keep this file <200 lines.

## 1. Project Snapshot
- **Name:** GitCthulhu
- **Stack:** <fill in>
- **Owner:** grego
- **Repo root:** `C:\Repos\GitCthulhu`
- **Bootstrapped:** 2026-06-30 via `agentforge`

## 2. Non-Negotiables

### 95% Confidence Rule
Before modifying ANY code or suggesting changes:
1. Read the actual files involved — do not assume content.
2. Trace the full call chain from entry point to effect.
3. If confidence is below 95%, state what's missing and ask. Never guess.

Be surgical: target the specific file/function/chain causing the issue. No cascading defensive changes.

### Determinism
If it can't be reproduced reliably across environments, it's broken. Flag non-determinism explicitly.

### Isolation Boundaries
Tenant logic, module boundaries, plugin scopes — any change crossing a boundary gets extra scrutiny and explicit justification.

## 3. Map (where things live)
<!-- Edit this section to point at your actual files. Reference, don't inline. -->
- **Entry point:** `@src/index.ts` <!-- e.g. -->
- **Config:** `@config/`
- **Types/contracts:** `@src/types/`
- **Tests:** `@tests/`
- **Build/CI:** `@.github/workflows/` or `@azure-pipelines.yml`
- **ADRs / decisions:** `@docs/adr/`

## 4. Build & Run
<!-- Replace with real commands. Keep terse. -->
```bash
<build>
<test>
<run>
```

## 5. Conventions
- British English in prose and identifiers where natural (favour, behaviour, analyse).
- No committed build artifacts. No fragile path aliases. No magic strings without constants.
- Code review format: **Verdict → Location → Issue → Impact → Remediation → Verification**.

## 6. Skills
Skill library is junction-linked at `.claude/skills/` → vault root. Reference a specific skill when invoking it; do not bulk-load.

## 7. Token Hygiene (operational)
- Use `/clear` when switching unrelated tasks.
- Disconnect MCP servers not needed for the current task (~18k tokens each).
- Batch prompts; edit + regenerate instead of follow-up corrections.
- Use Plan Mode for non-trivial work.
- Run `/context` and `/cost` periodically.
- Compact at ~60% context, not auto-trigger at 95%.
- Run `/compact` or `/clear` before breaks >5 min (prompt cache timeout).
- Surgical file refs (`@path/to/file.ts:fn`), never "search the repo".
- Limit shell output: `head`, `tail`, `--oneline`, `-n`. Deny heavy commands in permissions.
- Sonnet by default. Haiku for sub-agents and formatting. Opus only for deep architecture (<20% usage).

## 8. Lessons Learned
<!--
When something fails repeatedly, when I have to re-explain, or when a workaround is found
for a platform, tool, or limitation, add a one-line bullet here.
Keep each bullet under 15 words. No explanations. Only add things that will save time in future sessions.
Review weekly — prune anything stale.
-->
- _(empty — populate as patterns emerge)_

## 9. Decisions Log
<!-- Stable architectural decisions. One line each. Link to ADR if it deserves more. -->
- ADR 0001: window does not name the git adapter.
- ADR 0002: app state stays a reducer.
- ADR 0003: sessions-first GitLens path — worktree tabs own `AppState`; compare + split diff after WP1. Spec: `docs/superpowers/specs/2026-10-25-worktree-tabs-design.md`.
- ADR 0004–0006 (accepted 2026-10-27): conflict resolver — .NET + Avalonia/AvaloniaEdit behind `IEditorSurface`; git CLI authoritative; immutable merge plan + anchored result. Spec: `docs/superpowers/specs/2026-10-27-conflict-resolver-design.md`.

