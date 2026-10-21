# ADR 0001: The window does not name the Git adapter

Status: Accepted, amended by the `app` crate

## Context

`gui_app` was constructing `GitoxideAdapter` itself, so the `GitRepository` port was bypassed and the window could not be tested with a fake. Candidate B of the 30 June 2026 architecture review closed that seam. Five decisions were locked before the change.

## Decision

1. **Startup.** Remember the last repository, and otherwise show the welcome screen. `GitApp` holds an optional repository port. `None` is the welcome screen.
2. **Ownership.** `gui_app` does not depend on `git_adapter`. Only the composition root constructs the concrete adapter.
3. **Composition root.** Wiring lives outside the window library.
4. **Blocking.** `GitRepository` stays synchronous. The window runs a call on a background thread inside `iced::Task::perform`.
5. **Tests.** `test_support::FakeGitRepository` is the shared fake. `gui_app` uses it as a dev-dependency.

## What shipped

Decision 2 is compiler-enforced. `gui_app/Cargo.toml` depends on `core_domain` and `iced` only. The binary is the `app` crate (`git_cthulhu`), not a `[[bin]]` inside `gui_app`. A library and binary in one crate could not both satisfy “the library drops the dependency” and “the binary constructs the adapter”.

Decision 4 uses `std::thread` inside `Task::perform`, not `tokio::task::spawn_blocking`. Refresh, checkout, stage, unstage, and commit all use that path. Open Repository runs the folder dialog on that same kind of thread, then calls a factory supplied by `app`.

Decision 1 stores the opened path on `AppSettings::last_repo_path` through `config_adapter`. A command-line path wins. With no path, startup opens the saved folder only when it is still a repository; otherwise the welcome screen opens. `watch_settings` is not called.

## Consequences

A new Git operation is a method on `GitRepository`, an adapter implementation, a `Message` in `gui_app`, and a background job. The window tests use the fake and do not open a repository on disk.

`git` must be on `PATH` for checkout and for worktree edits. Those commands run the user’s hooks and signing.
