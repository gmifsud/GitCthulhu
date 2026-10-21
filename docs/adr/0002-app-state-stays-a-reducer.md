# ADR 0002: AppState stays a reducer

Status: Accepted

## Context

Candidate C proposed splitting `AppState` so the state machine returned a `SideEffect` for every user action. The original friction was `handle_token_received` writing to the keyring inside the reducer, and every port living in one file.

## Decision

`AppState::apply_event` stays a pure reducer of `ViewEvent`. The window chooses the next background task. `loading` is set by `GitApp::update` when work starts, and cleared when a view event arrives.

No `SideEffect` enum.

## Why

Refresh, checkout, and worktree edits already follow that path. A side-effect list would be rewritten when opening a repository and again when fetch exists. The keyring write that motivated the split has been removed. Models, ports, and state already live in separate modules.

## Consequences

A new user action is a `Message` and a task in `gui_app`, not a new variant the domain has to schedule. `apply_event` remains safe to call in tests with no adapter present.
