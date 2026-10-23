# GitCthulhu

Desktop Git client. The window is iced. Git work goes through a port; one adapter implements it.

## Terms

**Port.** `GitRepository` in `core_domain`. Sync methods. The window depends on this interface, not on gitoxide.

**Adapter.** `GitoxideAdapter` in `git_adapter`. Reads history, branches, and status with gix. Checkout, stage, unstage, and commit go through the `git` executable, because gix 0.66 keeps the worktree options those commands need private.

**Composition root.** The `app` crate. The only module that constructs `GitoxideAdapter` and `ConfyConfigStore`, and passes the repository port into `gui_app::run`. `gui_app` does not depend on `git_adapter` or `config_adapter`.

**Welcome screen.** `GitApp` with no adapter. Shown when there is no command-line path and no remembered repository that can still be opened. Open Repository asks for a folder, then the composition root's opener builds the port and stores that path.

**Repository view.** `GitApp` holding `Arc<dyn GitRepository + Send + Sync>`. Shown when a path was passed on the command line, after Open Repository succeeds, or when the remembered folder is still a repository. Loads status, the newest 50 commits from every local branch, and branches as soon as the window has a repository.

**View event.** `ViewEvent`. The only input to `AppState::apply_event`. Status, log, branches, settings, and errors. The reducer does not call an adapter or choose the next task. The window does.

**Worktree edit.** Stage, unstage, or commit of whole paths. Runs on a worker thread, then refreshes the repository view.

**Fake repository.** `test_support::FakeGitRepository`. A port implementation whose results are set by the test. Used by `gui_app` tests. Not a dependency of the running binary.

## What a session can do

Open a repository from the command line or from the welcome screen's folder dialog. Init a folder that is not already a repository, or clone a URL into a new directory under a chosen parent, then open that path the same way. Clone uses the user's `git clone` and does not sign in to a provider. Reopen the last of those paths on the next launch when it is still a repository. The header shows that path and an Open… control, so a second folder replaces the one on screen. Show branches, a 50-commit graph of every local branch (and a detached HEAD), the changes list, and one line for the current branch: its upstream and how many commits it is ahead or behind. Check out a local branch. Stage, unstage, and commit whole paths. Fetch the default remote, push the current branch to its upstream, and pull with `--ff-only`, using the user's git. Stash tracked changes from the branch ribbon, and apply, pop, or drop one stash from the refs list. Drop asks first. Untracked files stay in the worktree. Create a branch at the selected tip without checking it out, rename a local branch, and delete a merged branch that is not current. Delete asks first. An unmerged branch is left, with git's own message. Tags that point at a commit appear in the refs list, and only when there is at least one. Selecting a tag reads that commit's patch. The refs, history, and changes columns can be dragged wider or narrower. Refs and changes can each collapse to a rail on that side and open again at the width they had. A commit can be cherry-picked onto the current branch, or the current branch can be reset to it: soft, mixed, or hard. Hard discards the worktree. Another local branch can be merged with `--no-ff`, or the current branch can be rebased onto it. Each of those asks first. A conflict shows git's message and reloads the repository.

## Later: several repositories and worktrees

One window still holds one `GitRepository`. `last_repo_path` is a single path. A later session can keep a list of paths in the composition root and open one of them through the same opener. A git worktree is another path (`git worktree list`); it opens the same way. The window does not grow a second adapter, and `gui_app` still does not name one.

## Later: the changes list

The changes column is still one row per path. A later cut makes it closer to GitKraken: several paths selected at once, drag between unstaged and staged, stage all, unstage all, discard, delete, and stop tracking. The same paths can be shown as a tree or as a flat list.

## Not in the session yet

Fetch, push, and fast-forward pull use the user's git and take no credential argument. A pull that is not a fast-forward is refused. A conflict during cherry-pick, merge, or rebase is not listed file by file yet. The settings watcher is not started. The other adapters (`keyring`, `cache`, `ssh`, `auth`) implement ports and are not constructed by the composition root.
