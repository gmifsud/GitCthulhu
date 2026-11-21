# GitDesktop competitive analysis and adoption approach

Date: 2026-11-21 · Status: proposal for review · Supersedes the "Ordered priorities (revised)" list in [`../proposals/gitdesktop-transfer.md`](../proposals/gitdesktop-transfer.md). That brief remains the stack contrast and transfer rule.

**Reference build:** `D:\Repos\Tools\GitDesktop` at `119c5d25` (theBGuy/GitDesktop, Apache-2.0, Tauri 2 + React 19). The October brief analysed an older fork (`D:\Repos\_ref_GitDesktop`). Since then GitDesktop has grown a large forge, AI and agent surface. Its local-git core has also deepened: line staging, edit history, an operation journal, lost-stash recovery and sync guards.

**Method:** GitDesktop's README catalogue was read in full. Its frontend (`src/features`, `src/lib`) and Rust backend (`src-tauri/src/git`) were traced for each local-git feature. GitCthulhu was inventoried in the same way (port surface, `gui_app`, the open proposals). Spot checks confirmed the gaps that matter most. Other competitors are assessed from public knowledge only, not from their code. Their columns are marked accordingly.

---

## 1. Landscape

| Product | Position | Why it matters to us |
|---|---|---|
| **GitDesktop** (primary) | Free, Apache-2.0, Tauri/React. Keeps GitHub Desktop's model, then adds a deep local-git layer and a large forge/AI layer. | It is the closest open-source reference, and its source is readable. Its local-git behaviour is the feature bar to clear. |
| **GitHub Desktop** (upstream baseline) | Free, Electron. Simple on purpose and GitHub-centric. | It defines what users expect from the basics. GitDesktop's README lists GitHub Desktop's gaps (interactive rebase stops at squash/reorder, the merge dialog has no `--no-ff` or `-X`), and those gaps are its pitch. |
| **GitLens / GitKraken** | Commercial; editor extension plus a desktop app. | ADR 0003 names the GitLens path (sessions, compare, split diff) as GitCthulhu's direction. GitKraken is GitDesktop's anti-reference for visual noise. |
| **Fork / Sublime Merge** | Paid native apps, fast and dense. | They are GitCthulhu's true peers on **native performance**: syntax highlighting, blame, file history and interactive rebase without web-view lag. |
| **Zed git panel** | Editor-integrated. | GitCthulhu's history drawing already follows it (commits `375328b`, `3912593`). |

---

## 2. Feature comparison matrix

Legend: ✓ present · ◐ partial · ✗ absent. The GitHub Desktop and Fork/Sublime Merge columns are from public knowledge, not code review.

| Feature | GitCthulhu today | GitDesktop | GitHub Desktop | Fork / Sublime Merge |
|---|---|---|---|---|
| Open / clone / init | ✓ (clone with progress + cancel) | ✓ (+ README/licence scaffolding, locate-moved-repo) | ✓ | ✓ |
| Worktree-first workflow | **✓ tabs, colours, WIP rows per worktree** | ◐ manager dialog | ✗ | ◐ |
| Changes list/tree, folder collapse | ✓ | ✓ compacted chains + arrow-key folding | ◐ list | ✓ |
| Changes filter (path + category) | ✗ | ✓ | ◐ | ✓ |
| Per-file `+n −m` in changes list | ✗ (commit detail only) | ✓ numstat | ✗ | ✓ |
| Hunk stage / discard | ✓ | ✓ | ✓ | ✓ |
| **Line staging** | ✗ | ✓ drag gutter, Ctrl+Shift+Enter | ✓ click lines | ✓ |
| Commit body (multi-line) | ✗ single line | ✓ | ✓ | ✓ |
| **Amend / undo last commit** | ✗ | ✓ with expected-HEAD guard | ✓ | ✓ |
| Co-authors | ✗ | ✓ suggested from history | ✓ | ◐ |
| Split + unified diff, word highlight | ✓ | ✓ | ✓ | ✓ |
| Diff find (Ctrl+F) | ✓ | ◐ | ✗ | ✓ |
| Syntax highlighting | ✗ | ✓ | ✓ | ✓ |
| Image diff | ✗ | ✓ | ✓ | ✓ |
| Graph with lanes and ref pills | **✓ curved lanes, Refs column** | ◐ list, no graph lanes | ✗ | ✓ |
| History search | ✓ loaded + branch/user filter | ✓ + `--grep` across all history | ◐ | ✓ |
| Unpushed marker per commit | ◐ counts only | ✓ | ✓ | ✓ |
| **File history / blame** | ✗ | ✓ `log --follow`, `blame --porcelain` | ◐ | ✓ |
| Merge with preview | **✓ merge-tree preview, `--no-ff`** | ✓ + `-X ours/theirs`, squash | ◐ count only | ◐ |
| Rebase onto | ✓ | ✓ "Change base" with move preview | ◐ | ✓ |
| **Interactive rebase** | ✗ | ✓ atomic, unpushed only | ◐ squash/reorder | ✓ |
| Cherry-pick / revert | ◐ cherry-pick only | ✓ incl. onto another branch | ✓ | ✓ |
| Pull modes | ◐ ff-only | ✓ ff-only / rebase / merge, rebase-drop precheck | ◐ | ✓ |
| Force push (lease + if-includes) | ✗ | ✓ | ✓ lease | ✓ |
| Publish branch | ✗ | ✓ incl. without switching | ✓ | ✓ |
| Auto-fetch | ✗ | ✓ while focused | ✓ | ✓ |
| Autostash around operations | ✗ | ✓ six named outcomes + backup stash | ◐ on switch | ◐ |
| Stash browse / create / apply | ✓ | ✓ | ◐ one per branch | ✓ |
| **Lost-stash recovery** | ✗ | ✓ `fsck` dangling stash commits | ✗ | ✗ |
| **Operation journal** | ✗ | ✓ crash-interrupted ops named | ✗ | ✗ |
| Branch cleanup (merged/idle) | ✗ | ✓ archive or delete | ✗ | ◐ |
| Tags create/delete/push | ◐ read-only | ✓ | ◐ | ✓ |
| Submodules | ✗ | ✓ manager | ◐ | ✓ |
| Remotes management | ✗ | ✓ | ✗ | ✓ |
| Conflict banner continue/abort | ✓ | ✓ | ✓ | ✓ |
| Conflict editor | **◐ Rider-style three-way app (separate process)** | ✓ two-way region accept in-window | ✗ external | ✓ |
| Compare two refs | ✗ (Phase 5 open) | ✓ three-dot diff, ahead/behind commits | ✓ | ✓ |
| Command palette that runs commands | ◐ quick-open only | ✓ ~200 actions | ✓ | ✓ |
| Rebindable shortcuts + cheat sheet | ✗ | ✓ | ✗ | ◐ |
| Disabled-with-reason | ✗ | ✓ everywhere | ◐ | ✗ |
| Typed errors + details | ◐ status bar expand | ✓ kind + summary + details | ◐ | ◐ |
| Integrated terminal | **✓ pwsh pane** | ✓ task/agent terminal | ✗ | ✗ |
| Themes | ◐ dark only | ✓ 4 themes | ✓ | ✓ |
| Window / layout memory | ◐ tabs only | ✓ geometry validated against monitors | ✓ | ✓ |
| Git config, hooks editor | ✗ | ✓ | ◐ | ◐ |
| Forge PRs / issues / CI | ✗ | ✓ GitHub, GitLab, Bitbucket | ◐ GitHub | ✗ |
| AI (commit / review / conflicts) | ◐ suggest-commit | ✓ multi-provider | ◐ Copilot commit | ✗ |

**What the matrix says.** GitCthulhu already **leads** on graph, worktrees, merge preview, diff find, terminal and native performance. It is **behind on the daily commit loop**: body, amend, undo and line staging. It is also behind on **sync richness**, **history archaeology** (blame, file history, interactive rebase) and **safety and recovery** infrastructure.

---

## 3. UX approach summary

| Aspect | GitDesktop | GitHub Desktop | Fork / Sublime Merge | GitCthulhu |
|---|---|---|---|---|
| Onboarding | Welcome screen: Open / Clone / Create with live key hints, drag a folder in, recents grouped by owner | Sign-in first, then repo list | Open repo, recents | Repositories column + clone window; no first-run guidance |
| Core interaction | Tabs (Changes / History / …) with a sidebar list beside a diff | Two tabs: Changes, History | Graph-centric, commit panel at the bottom | Four-pane GitLens shell: Graph · Inspector · Diff · Files, terminal below |
| Information architecture | Three primary tabs + "More ▾", ~200 palette actions | Minimal | Dense, many panels | Explorer tree (Local/Remote/Tags/Stashes/Worktrees) + worktree tabs |
| Visual language | "Calm, precise, dependable"; restrained shadcn palette; four themes | Familiar GitHub look | Dense, native | Deep Focus orange tokens; one dark theme |
| Notable strength | Every disabled control says why; every destructive path re-checks state after the confirm | Approachability | Speed | Graph legibility, per-worktree colours |
| Notable frustration | Breadth: about 4,000-line components and a very long settings surface | Missing power features | Price / paid licence | Missing daily-loop basics (amend, body, line staging) |

---

## 4. Technical trade-offs

| Concern | GitDesktop | GitCthulhu | Take-away |
|---|---|---|---|
| Git access | System `git` subprocess, NUL-separated formats | System `git` via `run_git`, behind the `GitRepository` port | GitDesktop's **commands port directly** into new port methods |
| Concurrency | Three lock domains per repo (working tree, network, worktree-admin); busy errors name the holder ("a pull") | Background jobs, at most two repository reads at once | Adopt **named holders** and the **working-tree vs network split**, so staging keeps working while a fetch runs |
| Progress / cancel | Spinners only; no cancellation of git operations | Streamed clone progress + cancel | **We are ahead here**; extend streaming to fetch, pull and push |
| Rendering | Web view, virtualised lists, render caps (200 lines, 4,000-char lines, 20k-char minified guard) | iced, O(visible) per ADR 0007 | Borrow the **caps and placeholders** (minified-line guard) |
| Safety | Expected-HEAD / expected-tip checks, atomic temp+rename journal, recycle-bin discards | Confirm overlays | Adopt the **guard contract** in the port (see A3) |
| Licence | Apache-2.0 | — | Port behaviour freely; if any code is translated, keep attribution per its `NOTICE` |

---

## 5. Differentiation opportunities

1. **Worktree-native client.** No competitor treats worktrees as tabs with stable colours across graph, explorer and WIP rows. Push this further: per-worktree sync state in tabs, and "promote worktree branch" (GitDesktop has it as a dialog only).
2. **A real three-way resolver.** GitDesktop's editor accepts current, incoming or both per region. Cthulhu Resolve (ADR 0004–0006) does anchored three-way merges with an accessible navigator. Finishing Phase 1 and launching it from the conflict banner beats every peer in this list except Fork and Sublime Merge.
3. **Native speed at scale.** GitCthulhu is tested at 1,100 branches and 16 worktrees and holds O(visible). GitDesktop's switcher re-sorts and fetches divergence per open. Speed is the reason to switch from Electron or Tauri clients, so every adopted feature must keep ADR 0007.
4. **Cancellable, streamed long operations.** GitDesktop has none. Clone already streams here. Making fetch, pull, push and rebase streamable and cancellable is a visible advantage.
5. **History you can interrogate.** We have the best graph of the set. Adding blame and file history *from the graph* ("history of this file from this commit") is a loop that GitDesktop runs through dialogs.

---

## 6. Lessons from prior art

- **Adopt: one action catalogue** behind the palette, shortcuts and sheet (GitDesktop `src/lib/hotkeys/registry.ts`). It is proven in GitDesktop, Sublime Merge and VS Code, and users expect Ctrl+K / Ctrl+Shift+P.
- **Adopt: disabled with a reason.** A grey button with no explanation is the most common GUI-git frustration. GitDesktop applies the rule universally (`use-disabled-reason.ts`).
- **Adopt: re-check after the confirm.** For example, "HEAD moved while the dialog was open — nothing was reset". This matters more for us because of multiple worktrees and the terminal pane: state can change under an open dialog.
- **Adopt: line staging, amend and undo commit.** These are table stakes. Every peer except a bare CLI has them.
- **Adopt: named autostash outcomes.** Users fear losing work. Six explicit outcomes plus a kept backup stash removes that fear.
- **Avoid: feature-surface sprawl.** GitDesktop's `BranchSwitcher.tsx` is about 4,000 lines and its README about 1,400. The transfer rule stands: no forge, AI-review, agent, MCP or automations work until local parity.
- **Avoid: blocking dialogs for long work.** GitDesktop learnt this and now keeps the branch list open with per-row push progress. Use the status bar, toasts and per-row progress instead.
- **Avoid: "recent" heuristics that move things.** GitCthulhu already reversed this for the repository sidebar (backlog item 1). Keep ordering user-controlled.
- **Innovate: interruption recovery.** GitDesktop's journal is inform-only. We could pair it with the reflog to offer a one-click, guarded "return to where you started".
- **Innovate: compare as a first-class mode.** No peer ties compare, conflict prediction and the resolver into one flow. Phase 5 plus `merge_preview` plus Resolve gives us that.

**Table stakes vs differentiators.** Table stakes: commit body, amend, undo, line staging, pull modes, force-with-lease, publish, blame, file history, palette, syntax highlighting. Differentiators: worktree-first workflow, three-way resolver, native scale, cancellable streaming, operation journal, lost-stash recovery.

---

## 7. Adoption approach

### Principles

- **Behaviour, not code.** Each item names the GitDesktop file to study and the GitCthulhu landing site. Implementation stays in iced views, `Message` → reducer (ADR 0002), with git behind `GitRepository` (ADR 0001).
- **Port first.** Each feature begins as a port method with adapter tests against a scratch repository (`test_support`), then the view.
- **ADR 0007 gate.** No feature may add a per-row scan or a per-pointer message. Measure with `perf_harness` / `GITCTHULHU_PERF=1` on the Backend repo before merging.
- **One wave at a time.** Waves are ordered by dependency and daily pain. Items inside a wave are mostly independent.

Sizes: S ≤ 1 day · M 2–4 days · L ≥ 1 week.

### Wave A: foundations (the enablers the other waves need)

| # | Item | GitDesktop reference | GitCthulhu landing | Port | Size |
|---|---|---|---|---|---|
| A1 | **Action catalogue + command palette.** One `ACTIONS` table (`id`, label, category, default key, `enabled(&GitApp) -> Result<(), Reason>`). `shortcut_message` / `typing_shortcut` become lookups. The palette gains a command mode (`>` prefix or Ctrl+Shift+P) listing only enabled actions. Add a shortcuts sheet. Rebinding can wait. | `lib/hotkeys/registry.ts`, `handler-registry.ts`, `CommandPalette.tsx` | new `gui_app/src/actions.rs`; `palette.rs`, `view/palette.rs`, `app.rs` ≈5718–5887 | — | M |
| A2 | **Disabled-with-reason.** The `enabled()` result drives the button tooltip and the menu suffix ("Push (no upstream)"). | `use-disabled-reason.ts` | `view/top_bar.rs`, `context_menu.rs` | — | S |
| A3 | **Guard contract.** Destructive port calls take `expected_head: Option<&str>` (reset, undo, amend, discard-all, rewrite). The adapter refuses with `DomainError::Moved`. | `use-refuse-if-moved.ts`, `ops.rs` expected-tip checks | `core_domain/src/ports.rs`, `git_adapter` | yes | S |
| A4 | **Typed error presentation.** Classify stderr into a kind (Conflict, Busy{holder}, Rejected, Auth, Transport, DirtyTree, PathTooLong), a one-line summary and details. The status bar shows kind + summary; Details expands. | `lib/error-summary.ts` | `core_domain` error type, `view/status_bar.rs`, `activity.rs` | yes | M |
| A5 | **Persist layout.** Window geometry (validated against monitors), preset, split ratios, diff split, tree mode. This is three-column Phase 6. | `tauri-plugin-window-state` | `AppSettings`, `layout.rs` | — | S |

### Wave B: the daily commit loop (table stakes)

| # | Item | GitDesktop reference | GitCthulhu landing | Port | Size |
|---|---|---|---|---|---|
| B1 | **Commit summary + body**, Ctrl+Enter commits; Enter in the summary moves to the body. | `features/commit/CommitBox` | `view/changes.rs` (`text_editor` for body) | `commit(summary, body)` | S |
| B2 | **Amend + undo last commit.** Amend pre-fills the message. Undo = `reset --soft HEAD~1`, guarded by A3, and keeps the message as a draft. | `useCommitSubmit`, Ctrl+Z | `view/changes.rs`, `app.rs` | `commit(.., amend)`, `undo_commit(expected_head)` | M |
| B3 | **Changes filter**: path substring + chips (Staged, Unstaged, New, Modified, Deleted). | `ChangesPanel.tsx` filter | `changes.rs` (`change_rows`), `view/changes.rs` | — | S |
| B4 | **Per-file `+n −m`** in the changes list via `diff --numstat -z` (and `--cached`), loaded with status. | `src-tauri/src/git/diff.rs` ~L999 | `git_adapter/src/status.rs`, `changes.rs` | `numstat()` | S |
| B5 | **Compacted tree + arrow keys.** Merge single-child folder chains; ←/→ fold, unfold and step to the parent; Shift+↑/↓ extends the selection. | `lib/path-tree.ts` `flattenPathTree` | `changes.rs` | — | S |
| B6 | **Line staging.** Drag across the gutter (reuse `diff_select.rs`'s drag model). Ctrl+Shift+Enter stages or unstages the selection. The adapter builds a partial patch (unselected `+` dropped, unselected `−` → context; reverse mirrors) → `git apply --cached --recount`. Refuse when the diff is truncated or not UTF-8. | `diff.rs` `build_partial_patch` | `diff_select.rs`, `view/patch.rs`, `git_adapter` | `stage_lines(path, selection, reverse)` | L |
| B7 | **Graph ↑/↓ selection** (three-column Phase 6). | `lib/list-keyboard-nav.ts` | `view/history.rs`, `history_index.rs` | — | S |

### Wave C: sync you can trust

| # | Item | GitDesktop reference | GitCthulhu landing | Port | Size |
|---|---|---|---|---|---|
| C1 | **Push states.** "Publish branch" without an upstream (`push -u`); "Force push…" when diverged, using `--force-with-lease --force-if-includes` (lease-only on git < 2.30 or with no reflog), confirmed, naming which guard applied. | `SyncControls.tsx`, `git/remote.rs` | `view/top_bar.rs`, `git_adapter/src/lib.rs` ~L761 | `push(mode)` | M |
| C2 | **Pull modes**: ff-only default, plus rebase and merge from a menu. Fetch under the network half, then integrate on the pinned SHAs. **Rebase-drop precheck** (`merge-base --fork-point`) names the commits that would vanish: Keep (default) or Drop, with a stale-decision guard. | `git/pull_guard.rs`, `api/sync.ts` | `git_adapter`, `view/top_bar.rs` | `pull(mode, decision)` | L |
| C3 | **Autostash compound.** Stash `-u` → operation → pop, with six named outcomes. A conflicted reapply sets a marker; the conflict banner then offers "drop backup stash" or "keep". Applies to pull, merge, rebase and switch. | `git/autostash.rs`, `useStashReapplyRecovery.tsx` | `jobs.rs`, `view/operation.rs` | `with_autostash(op)` | M |
| C4 | **Switch with changes dialog**: Bring changes / Stash and switch. | `SwitchWithChangesDialog.tsx` | `view/branch_popup.rs`, `view/confirm.rs` | uses C3 | S |
| C5 | **Auto-fetch while focused**: an interval plus on-focus-if-due; skip when busy or no remote; quiet. ADR 0007: post a `ViewEvent` only when refs actually changed. | `lib/git/auto-fetch.ts` | `jobs.rs`, `AppSettings` | — | S |
| C6 | **Lock domains with named holders**: working-tree vs network, so staging stays live during a fetch; `Busy{holder}` feeds A4. | `git/runner.rs` | `jobs.rs` | — | M |
| C7 | **Streamed, cancellable fetch/pull/push** (our differentiator; reuses `clone_with_progress`'s pattern). | — (GitDesktop lacks it) | `git_adapter`, `jobs.rs`, status bar | yes | M |

### Wave D: history archaeology

| # | Item | GitDesktop reference | GitCthulhu landing | Port | Size |
|---|---|---|---|---|---|
| D1 | **Unpushed marker per graph row**: commits in `@{u}..HEAD`, gated on the graph head matching status HEAD. | `HistoryPanel.tsx` | `history_index.rs`, `view/history.rs` | `unpushed_ids()` | S |
| D2 | **Search all history**: `log -i -F --grep` / `-S` beyond the loaded pages. | `git/history.rs` | `view/filter_bar.rs`, `log_filter.rs` | `search_log()` | M |
| D3 | **File history** (`log --follow -- <literal path>`) from any file row (changes, commit files, Compare). It shows as a filtered graph rather than a dialog. | `FileHistoryDialog.tsx` | `log_filter.rs`, `context_menu.rs` | `file_log()` | M |
| D4 | **Blame** (`blame --porcelain`) as a Diff-pane mode. Hovering a line shows a commit card; clicking it selects that commit in the graph. | `BlameDialog.tsx` | new `view/blame.rs`, `diff_view.rs` | `blame(path, rev)` | L |
| D5 | **Revert commit** from the graph context menu. | ops `revert` | `context_menu.rs` | `revert(id)` | S |
| D6 | **Compare two refs** (three-column Phase 5): ahead/behind commit lists + three-dot file diff, with per-commit menu (cherry-pick, branch, copy SHA). | Compare tab | `compare.rs`, `view/compare.rs` | `compare(base, head)`, `range_patch()` | L |
| D7 | **Edit history (interactive rebase)**: unpushed commits only, cap 15. pick/reword/squash/fixup/drop/edit; reorder with Alt+↑/↓. Atomic replay through `GIT_SEQUENCE_EDITOR`, with a plan-coverage check and a start-tip gate. Any failure → `rebase --abort` back to the original tip. `edit` leaves a resumable rebase for the conflict banner. | `EditHistoryDialog.tsx`, `ops.rs` ~L5113 `git_rewrite_commits` | new `view/edit_history.rs` | `rewrite_commits(plan, expected_tip)` | L |
| D8 | **Change-base preview**: list `old..HEAD` before `rebase_onto`. | `RebaseOntoDialog.tsx` | `view/confirm.rs` | `log_range()` | S |

### Wave E: recovery and hygiene

| # | Item | GitDesktop reference | GitCthulhu landing | Port | Size |
|---|---|---|---|---|---|
| E1 | **Operation journal**: record compound operations (rewrite, cherry-pick-onto, pull-rebase drop, autostash) with the original ref and SHA. Atomic writes, best-effort (never fail the git operation). On open, reconcile and show an inform-only banner. Optional innovation: a guarded "return to start". | `src-tauri/src/oplog.rs`, `OpRecoveryBanner.tsx` | `config_adapter` (next to `ActivityStore`), `view/operation.rs` | `OperationJournal` port | M |
| E2 | **Lost-stash recovery**: `fsck --no-progress` dangling commits, minus the live stash list, filtered to stash-shaped commits (≥2 parents, `WIP on `/`On `). Restore via `stash apply <sha>`. | `ops.rs` ~L2778 | Explorer Stashes group "Recoverable…" | `orphaned_stashes()` | S |
| E3 | **Branch cleanup**: merged into default (ahead 0) or idle 30/60/90 days. Archive (hidden via config flag) or delete. Never the current branch, the default, or a branch checked out in another worktree. | `CleanupBranchesDialog.tsx` | new dialog; `ref_tree.rs` hides archived | `archive_branch`, `delete_branch(force)` | M |
| E4 | **Tags**: create (lightweight/annotated), delete, push. | `features/tags` | `view/explorer.rs` | `create_tag`, `delete_tag`, `push_tag` | S |
| E5 | **Worktree lock/unlock + promote** (already read; add the write). | `WorktreesDialog.tsx` | `view/worktrees.rs` | `lock_worktree` | S |
| E6 | **Conflict banner → Cthulhu Resolve** per file; keep take-ours/theirs for binaries. This depends on resolver Phase 1 sign-off (the hand check is still open). | `ConflictBanner.tsx` | `view/operation.rs`, `run_merge_tool` | exists | S |

### Wave F: breadth (as capacity allows)

- **Syntax highlighting** in Diff (iced `highlighter` / syntect). Highlight only the visible slice, cached per hunk. ADR 0007 measurement required. **M–L**
- **Image diff** (old/new with dimensions; withhold above 20 MB). **M**
- **Minified-line guard** (>20k chars → placeholder) and long-line caps. **S**
- **Light theme** from `design/system` tokens (the token pipeline already exists via `design_sync`). **M**
- **Git settings** (identity global/local, `init.defaultBranch`, `core.autocrlf`) and **remotes management**. **M**
- **Open in editor / external terminal** integration. **S**
- **Hooks viewer** (enable/disable by `.disabled` rename, honour `core.hooksPath`). **M**
- **Submodules** (status first; manager later). **L**
- **Co-authors** suggested from `shortlog -se`. **S**

### Explicitly not adopted (for now)

Forge PRs, issues and CI; AI review and security audits; coding-agent sessions; MCP in either direction; automations; the tasks runner (the terminal pane covers it); avatars (network fetch, privacy). The transfer rule is unchanged: these wait until Waves A–E are done. ADR 0003's GitLens path (compare, split diff) is served by D6.

---

## 8. Suggested sequencing

```
A1 ─┬─ A2                      (catalogue first: every later action registers there)
A3 ─┼─ B2, C1, C2, D7, E3      (guards before anything destructive)
A4 ─┴─ C6                      (busy holders need the error kinds)
B1 → B2 → B3/B4/B5 → B6        (commit loop; line staging last, it is the largest)
C1 → C3 → C2 → C4 → C5 → C7
D1/D5/D8 (quick wins) → D3 → D4 → D2 → D6 → D7
E2/E4/E5 (quick wins) → E1 → E3 → E6 (after resolver Phase 1)
```

A sensible first milestone, the **"daily driver" release**, is A1–A4 + B1–B5 + C1 + C4 + D1 + D5 + E2. All are S/M; together they close the most-felt gaps in about two to three weeks of focused work. Line staging (B6), pull modes (C2) and edit history (D7) come next as dedicated plans, one each, via `superpowers:writing-plans`.

## 9. Verification per item

- Adapter: a scratch-repo test in `git_adapter` per new port method, including the guard-refusal path (A3) and the git-version fallback (C1 lease-only).
- Reducer: `cargo test -p gui_app` for message → state, plus the `perf_harness` budget on any view change.
- Manual: the Backend repo (1,100 branches, 16 worktrees) on `target/debug/git_cthulhu.exe`. Every destructive path must be checked with state changed underneath (e.g. commit from the terminal pane while a confirm is open).

## Sources

- GitDesktop source and README at `D:\Repos\Tools\GitDesktop` (`119c5d25`).
- GitCthulhu inventory: `core_domain/src/ports.rs`, `gui_app/src/*`, `docs/proposals/three-column-window.md`, `docs/proposals/gitdesktop-transfer.md`.
- User demand signals: [GitHub Desktop 2.0: stashing and rebasing were the most requested features](https://github.blog/news-insights/product-news/github-desktop-expands-to-support-stashing-and-rebasing); [request for interactive rebase in GitHub Desktop](https://lightrun.com/answers/desktop-desktop-support-interactive-rebases); [Sublime Forum: interactive staging discussion](https://forum.sublimetext.com/t/interactive-staging/42885); [GitKraken feedback: more interactive-rebase commands](https://feedback.gitkraken.com/suggestions/203766/comment/448583), [split a commit](https://feedback.gitkraken.com/suggestions/191932/comment/655142).
- Market positioning: [Hostinger: best Git GUI clients 2026](https://www.hostinger.com/tutorials/best-git-gui-clients/); [SourceForge Git GUI comparison](https://sourceforge.net/software/git-gui-clients/).
