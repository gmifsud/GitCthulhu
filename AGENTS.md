# GitCthulhu — AGENTS.md

> Generic agent runner contract. Auto-discovered by Codex CLI, Cursor, Kilo, and other AGENTS.md-aware tools.
> Index, not data dump. Keep <200 lines.

## Project
- **Name:** GitCthulhu
- **Stack:** <fill in>
- **Owner:** grego
- **Repo root:** `C:\Repos\GitCthulhu`
- **Bootstrapped:** 2026-06-30 via `agentforge`

## Operating Principles

### 95% Confidence Rule
1. Read the actual files involved — do not assume content.
2. Trace the full call chain from entry point to effect.
3. If confidence <95%, state what's missing and ask. Never guess.

### Determinism
Reproducibility across environments is required. Flag non-determinism explicitly.

### Surgical Changes
Target the specific file/function causing the issue. No cascading defensive refactors.

## Build & Run
```bash
<build>
<test>
<run>
```

## Conventions
- British English where natural.
- No committed build artifacts; no magic strings without constants.
- Review format: **Verdict → Location → Issue → Impact → Remediation → Verification**.

## Skills
Skill library symlinked at `.agents/skills/` (or `.codex/skills/` depending on target). Reference specific skills; do not bulk-load.

## Token Hygiene
- Batch instructions in a single prompt where possible.
- Use surgical file references (`@path/file.ts`), never "search the repo".
- Limit shell output (`head`, `--oneline`, `-n N`).
- Prefer cheaper models for sub-agents and formatting tasks.

## Lessons Learned
<!-- One-line bullets, <15 words, prune weekly -->
- _(empty)_

## Decisions Log
<!-- Stable architectural decisions. Link ADRs where they exist. -->
- ADR 0001: window does not name the git adapter.
- ADR 0002: app state stays a reducer.
- ADR 0003: sessions-first GitLens path — worktree tabs own `AppState`; compare + split diff after WP1. Spec: `docs/superpowers/specs/2026-10-25-worktree-tabs-design.md`.
- ADR 0004–0006 (accepted 2026-10-27): conflict resolver — .NET + Avalonia/AvaloniaEdit behind `IEditorSurface`; git CLI authoritative; immutable merge plan + anchored result. Spec: `docs/superpowers/specs/2026-10-27-conflict-resolver-design.md`.

