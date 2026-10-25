# Branch focus + context menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Focus-current graph filtering, right-click Open-as-worktree (prefilled create) and Detach HEAD on commits/local branches, with thin port extensions.

**Architecture:** Per-session ephemeral focus flag filters the loaded DAG client-side. Context menu is an iced overlay fed by pure `context_actions`. Create modal gains optional `start_point`. New `checkout_detached` and `add_worktree` start tip run through existing path-routed finishes.

**Tech Stack:** Rust, iced 0.13, `core_domain` ports, `git_adapter`, `gui_app`, `test_support`.

## Global Constraints

- ADR 0001: `gui_app` never depends on / names `git_adapter`.
- ADR 0002: repository data enters via `AppState::apply_event` only.
- Path-routed finishes; WorktreeOpener without `remember`.
- British English in user-facing copy.
- Spec: `docs/superpowers/specs/2026-10-25-branch-focus-context-menu-design.md`.

## File map

| File | Responsibility |
|---|---|
| `core_domain/src/ports.rs` | `checkout_detached`; `add_worktree` + start tip |
| `git_adapter/src/lib.rs` | Implement both |
| `git_adapter/src/tests.rs` | Adapter coverage |
| `test_support/src/lib.rs` | Fake fields + trait |
| `gui_app/src/session.rs` | `focus_current_branch` |
| `gui_app/src/reachability.rs` (new) | Filter commits reachable from tip |
| `gui_app/src/context_menu.rs` (new) | Target, actions, availability |
| `gui_app/src/app.rs` | Messages, menu state, detach confirm, create prefill |
| `gui_app/src/jobs.rs` | Detach job; add_worktree start tip |
| `gui_app/src/view/history.rs` | Focus toggle, filter, right-click |
| `gui_app/src/view/refs.rs` | Local branch right-click |
| `gui_app/src/view/worktrees.rs` | Create start_point UI |
| `gui_app/src/view/mod.rs` | Context menu overlay |
| `gui_app/src/lib.rs` | `mod` exports |

---

### Task 1: Reachability filter + focus toggle

**Files:** `gui_app/src/reachability.rs`, `session.rs`, `lib.rs`, `app.rs`, `view/history.rs`

- [ ] Failing tests for reachable-from-tip (linear, merge, tip missing).
- [ ] Implement filter; `focus_current_branch` on session; `ToggleFocusCurrentBranch`.
- [ ] History chrome toggle; filter nodes; WIP only active when focused.
- [ ] `cargo test -p gui_app` focus/reachability; commit.

### Task 2: Port — checkout_detached + add_worktree start

**Files:** `ports.rs`, `git_adapter`, `test_support`, jobs fakes

- [ ] Failing adapter tests.
- [ ] Implement `checkout_detached`; extend `add_worktree(..., start_point: Option<&str>)`.
- [ ] Update all trait impls / call sites.
- [ ] `cargo test -p git_adapter -p test_support -p gui_app`; commit.

### Task 3: context_actions + overlay + right-click

**Files:** `context_menu.rs`, `app.rs`, `view/history.rs`, `view/refs.rs`, `view/mod.rs`

- [ ] Failing tests for enable/disable matrix.
- [ ] Implement helper + menu state; wire secondary click; Escape clears.
- [ ] `cargo test -p gui_app`; commit.

### Task 4: Prefill create + detach UI/jobs

**Files:** `app.rs`, `jobs.rs`, `view/worktrees.rs`, confirm chrome

- [ ] Create prompt `start_point`; `AskCreateWorktreeFrom`; pass to job.
- [ ] Detach ask/confirm/cancel; dirty gate; path-routed finish.
- [ ] Tests: prefill, dirty confirm, clean detach.
- [ ] Full sweep + clippy; commit.
