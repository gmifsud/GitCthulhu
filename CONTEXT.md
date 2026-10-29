# GitCthulhu

Desktop Git client. The window is iced. Git work goes through a port; one adapter implements it.

## Terms

**Port.** `GitRepository` in `core_domain`. Sync methods. The window depends on this interface, not on gitoxide.

**Adapter.** `GitoxideAdapter` in `git_adapter`. Reads history, branches, and status with gix. Checkout, stage, unstage, and commit go through the `git` executable, because gix 0.66 keeps the worktree options those commands need private.

**Composition root.** The `app` crate. The only module that constructs `GitoxideAdapter` and `ConfyConfigStore`, and passes the repository port into `gui_app::run`. `gui_app` does not depend on `git_adapter` or `config_adapter`.

**Welcome screen.** `GitApp` with no adapter. Shown when there is no command-line path and no remembered repository that can still be opened. Open Repository asks for a folder, then the composition root's opener builds the port and stores that path.

**Repository view.** `GitApp` holding one or more sessions, each with an `Arc<dyn GitRepository + Send + Sync>`. Shown when a path was passed on the command line, after Open Repository succeeds, or when the remembered folder is still a repository. Loads status, the newest 50 commits from every local branch, and branches as soon as the window has a repository.

**View event.** `ViewEvent`. The only input to `AppState::apply_event`. Status, log, branches, settings, and errors. The reducer does not call an adapter or choose the next task. The window does.

**Worktree edit.** Stage, unstage, or commit of whole paths. Runs on a worker thread, then refreshes the repository view.

**Fake repository.** `test_support::FakeGitRepository`. A port implementation whose results are set by the test. Used by `gui_app` tests. Not a dependency of the running binary.

## What a session can do

Open a repository from the command line or from the welcome screen's folder dialog. Init a folder that is not already a repository, or clone a URL into a new directory under a chosen parent, then open that path the same way. Clone uses the user's `git clone` and does not sign in to a provider. Reopen the last of those paths on the next launch when it is still a repository. The header shows that path and an Open… control, so a second folder replaces the one on screen. Show branches, a 50-commit graph of every local branch (and a detached HEAD), the changes list, and one line for the current branch: its upstream and how many commits it is ahead or behind. Check out a local branch. Stage, unstage, and commit whole paths. Fetch the default remote, push the current branch to its upstream, and pull with `--ff-only`, using the user's git. Stash changes (all, or the selected files; optionally with untracked files and a message), and apply, pop, or drop one stash from the branch tree. Drop asks first. Untracked files stay in the worktree. Create a branch at the selected tip, a remote branch, a tag or a commit, and check it out unless Checkout branch is cleared, rename a local branch, and delete a merged branch that is not current. Delete asks first. An unmerged branch is left, with git's own message. Tags that point at a commit appear in the refs list, and only when there is at least one. Selecting a tag reads that commit's patch. The refs, history, and changes columns can be dragged wider or narrower. Refs and changes can each collapse to a rail on that side and open again at the width they had. A commit can be cherry-picked onto the current branch, or the current branch can be reset to it: soft, mixed, or hard. Hard discards the worktree. Another branch can be merged (fast-forward when possible, or `--no-ff`), or the current branch can be rebased onto it. Each of those asks first. A conflict leaves the operation in progress with the conflicted files listed; see Merge and conflicts.

## Worktree sessions

**Session.** `WorktreeSession` in `gui_app`. One open worktree path with its own `GitRepository` port and its own `AppState` (status, commits, branches, stashes, tags) plus its UI state: selection, commit message, marked files, and the loaded diff. `GitApp` holds a list of sessions and the index of the active one. Switching tabs moves that index; it does not reload. A background job names the session path it started on, and its result is dropped if that tab has since closed.

**Tab strip.** Sits under the top bar, one tab per open session. Click to switch, close a tab with its close control (the last tab stays, and so does a tab with a job running). A `+` control opens a picker over the catalogue.

**Catalogue.** The list from `git worktree list --porcelain`, read through `GitRepository::worktrees` and shown in the Files column's Worktrees section. A linked worktree opens into a tab from there or from the picker. Create adds a worktree for a named branch at a chosen folder, creating the branch when asked. Remove asks first and has a force option for a dirty tree. The main worktree cannot be removed.

**Retention.** The setting for what a tab keeps when you leave it: full (message, marks, selection), partial (message and marks; selection cleared, the default), or path only (nothing but the path). Set in Settings and stored with the other settings.

**Opener.** `RepositoryOpener` is for Open… and remembers the folder as `last_repo_path`. `WorktreeOpener` is for tabs and does not, so opening a linked worktree never changes which repository the next launch reopens. The composition root builds both; `gui_app` names neither adapter.

**WIP rows.** The history graph shows one Working changes row for each open session that has staged, unstaged, or untracked paths, at the top in tab order. The pill carries that session's branch, or the folder name when HEAD is detached. Pressing a row switches to its session and shows its working tree. A worktree with no tab has no row.

## Finding things

**Log layout.** The default pane preset, after the JetBrains Git log: branch tree on the left, History in the middle with the Inspector beside it, and a full-width Diff below. The terminal starts collapsed. The top bar's layout control still cycles the other three presets.

**Branch tree.** The Explorer column. One filter box and one scroll over HEAD, Local, Remote, Worktrees, Stashes and Tags. Branch names fold into folders at each `/`; folders inside a remote start closed. A click filters the log to that branch, a double-click checks a local branch out, and every other action is on the row's right-click menu: new branch from a local or remote branch (checkout optional), checkout of a remote (creating the tracking branch), merge, rebase, rename, delete, and for worktrees open, new, open folder, copy path and remove.

**Log filter bar.** Above the History table: text search, Branch ▾ (All, HEAD, any local or remote branch), User ▾ (authors of the loaded commits) and the current-branch toggle. Dates are shown in UTC, because the crate reads no time zone. Merge commits are muted.

**Inspector.** A commit, tag or stash shows its changed files as a folder tree with line counts, with its details (hash, author, date, parents, refs) below. Any other selection shows the working tree, laid out like the JetBrains Commit window: a toolbar on top, then the files (the only part that scrolls) in Staged, Changes and Unversioned Files, then the commit message with Suggest and Commit. File names are coloured by change kind: blue modified, green added, grey deleted, red unversioned. A folder holding only one folder is joined to it on one row. The head names the selection and lists its commands as labelled buttons.

**Diff.** Side by side by default, after the JetBrains diff viewer. Removed and added runs are paired into modified rows (blue, with the changed words marked), unpaired lines are added (green) or removed (red) against an empty filler, and both gutters show line numbers. The head steps through differences (↑ ↓ n/N) and files (‹ n/N files ›) and switches to unified. `crate::diff_view` works the rows out before drawing, with fixed row heights, so jumping to a difference is a computed scroll offset.

**Branches popup.** The top-bar branch chip opens it, after the JetBrains Git branches popup: a search field, New Branch… (from HEAD, checkout ticked) and Merge into Current…, then the local and remote branches with ahead/behind. A branch opens its own menu: checkout, new branch from it, merge, rebase, rename, delete.

**Working-tree selection.** Click selects one file and shows its diff, Ctrl-click (Cmd on macOS) toggles a file, Shift-click selects the range from the last click in the order the files are drawn. The check boxes show the same selection. Right-click acts on the selection: stage, unstage, discard, stash, ignore, copy paths. Each counts only the files it applies to. Right-clicking outside the selection selects that row first. Discard asks once for everything selected, staged or not.

**Stash dialog.** Stash (top bar) and "Stash N files…" (right-click) ask for an optional message first. Stashing everything can include untracked files.

**Merge and conflicts.** Merge into Current… opens a dialog: pick the branch, and optionally always record a merge commit (`--no-ff`); otherwise git fast-forwards when it can. When files conflict, the merge stays in progress. `RepoStatus.operation` reports it, and a bar under the tabs says how many files still conflict, with Commit merge (enabled once none are left) and Abort merge. The same bar covers a stopped rebase, cherry-pick or revert with Continue and Abort. Conflicted files are listed under Merge Conflicts (`RepoStatus.conflicted_files`, never as staged deletions), each with Yours, Theirs and Resolved (after editing the file by hand), also on right-click.

**Change kind.** `RepoStatus.kinds` (`ChangeKinds` in `core_domain`) says how each staged or unstaged path changed: added, modified, deleted, renamed or conflicted. Untracked paths are unversioned. A path with no entry reads as modified, so an adapter that does not classify still reports something true.

**Icons.** Every icon comes from Codicons (`gui_app/assets/fonts/codicon.ttf`, CC BY 4.0), compiled into the binary and named by `icons::Icon`. Icons no longer depend on the fonts installed on the machine; a test checks every codepoint is in the font.

**Window resizing.** The Explorer keeps the pixel width you gave it when the window is resized (280 px to start, within 16–36 % of the width). The log and the Diff take up the difference.

**History search.** The log filter bar's text field (Ctrl+K) narrows the history table to loaded commits whose message, author, hash prefix, or tip ref name contains every typed term. Matches show their lane dot without lines, and a line under the headers says how many of the loaded commits matched. It searches what is loaded, not the whole repository.

**Quick open.** Ctrl+P opens one field over branches, remotes, worktrees, tags, stashes, changed files, loaded commits, and recent repositories, ranked by a subsequence score that favours word starts. A prefix scopes it: `b:` `t:` `s:` `w:` `c:` `f:` `r:`. Enter selects a branch, tag, stash, commit, or file (nothing is checked out), opens a worktree in a tab, or opens a repository. Esc, Ctrl+P, and Ctrl+K work while a text field has focus.

**Recent repositories.** `recent_repositories` in the settings, newest first, at most twelve. The repository opener and a command-line path add to it; worktree tabs do not. The welcome screen lists them and the top bar's Recent opens Quick open on `r:`.

### Later: several repositories

A session is a worktree of the repository that was opened. Unrelated clones in one window are not covered. `last_repo_path` is still the one path reopened at launch; the recent list only offers the others.

## Later: the changes list

The working tree can be a flat list or a folder tree. Click a path to look at it. Add marks more than one, Select all, or Clear. Stage and Unstage apply to the marked paths. Still later: drag between unstaged and staged, discard or delete several paths at once, and stop tracking.

## Not in the session yet

Fetch, push, and fast-forward pull use the user's git and take no credential argument. A pull that is not a fast-forward is refused. The settings watcher is not started. The other adapters (`keyring`, `cache`, `ssh`, `auth`) implement ports and are not constructed by the composition root.
