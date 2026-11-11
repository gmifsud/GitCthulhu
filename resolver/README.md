# Cthulhu Resolve

Standalone three-way Git conflict resolver.

- **Spec:** `../docs/superpowers/specs/2026-10-27-conflict-resolver-design.md`
- **Decisions:** ADR 0004–0006 in `../docs/adr/`

## Status: Phase 1 (merge-tool MVP)

Phase 0 (foundation and spikes) is done; its table is further down.

| Phase 1 item | State | Where |
|---|---|---|
| Command line and exit codes (`cthulhu-resolve merge …`) | ✅ | `src/Resolve.Core/Cli/CommandLine.cs` |
| Input loading, role labels (merge, rebase, cherry-pick, revert; stash roles are not detected yet) | ✅ | `Session/MergeInputs.cs`, `Session/OperationRoles.cs` |
| Planning from git's own diffs | ✅ | `Merge/MergePlanning.cs`, `Diff/GitLineDiff.cs` |
| Resolution document, region commands, undo | ✅ | `Resolution/ResolutionDocument.cs`, `Resolution/RegionCommands.cs` |
| Merge-tool session: atomic save, backups, exit codes, marker scan | ✅ | `Session/MergeToolSession.cs`, `Persistence/AtomicFileWriter.cs` |
| Editor surface and three panes with bands and synchronised scrolling | ✅ | `src/Resolve.App/Editor/`, `src/Resolve.App/Panes/` |
| Accessible conflict navigator (O1-A) | ✅ | `src/Resolve.App/Shell/ConflictNavigator.cs` |
| Merge-tool window, keys, confirmations | ✅ | `src/Resolve.App/Shell/` |
| Program entry (command line to window to exit code) | ✅ | `src/Resolve.App/Program.cs` |
| `git mergetool` end to end (acceptance 8), no display needed | ✅ | `tests/Resolve.MergeToolProbe/`, `tests/Resolve.Core.Tests/MergeToolAcceptanceTests.cs` |
| CI on Windows, macOS, Linux | Configured; not yet observed running for Phase 1 | `../.github/workflows/resolver.yml` |

**Known v1 limitation (decision O1-A):** screen-reader editing of the RESULT pane is not supported. The conflict navigator is accessible (keyboard, live announcements), but typing into RESULT with a screen reader is a v1 limitation.

## Use as git's merge tool

Build the app (`dotnet build src/Resolve.App -c Release`), then point git at the built `cthulhu-resolve`. On Windows the executable is `src/Resolve.App/bin/Release/net10.0/cthulhu-resolve.exe`; on Linux and macOS it is `cthulhu-resolve` in the same folder. On WSL use the Linux build, not the `.exe`, to keep path semantics consistent.

```ini
# ~/.gitconfig (Windows path shown, with <repo> your checkout; use forward slashes)
[merge]
    tool = cthulhu
[mergetool "cthulhu"]
    cmd = \"<repo>/resolver/src/Resolve.App/bin/Release/net10.0/cthulhu-resolve.exe\" merge --base \"$BASE\" --local \"$LOCAL\" --remote \"$REMOTE\" --merged \"$MERGED\"
    trustExitCode = true
[mergetool]
    keepBackup = false   # the resolver keeps its own backup before the first save
    prompt = false
```

`$BASE`, `$LOCAL`, `$REMOTE` and `$MERGED` are shell variables that git's shell expands, not environment variables, so they must appear in `cmd`. `trustExitCode = true` is required: git stages the file only when the exit code is 0 (resolved). Closing the window or choosing Cancel exits 1 and leaves the conflict in place.

Then run `git mergetool` (or `git mergetool -- <path>`). GitCthulhu's Compare window launches the resolver through `git mergetool`.

## Status: Phase 0 (foundation & spikes)

| Item | State | Where |
|---|---|---|
| Git runner | ✅ | `src/Resolve.Core/Git/GitRunner.cs` |
| Unmerged index reader, stage acquisition | ✅ | `Git/Unmerged.cs`, `Git/ConflictInputs.cs` |
| Text codec | ✅ | `Text/TextCodec.cs` |
| Line diffs (Myers, Histogram) + git's diffs | ✅ | `Diff/` |
| diff3 planner + marker renderer | ✅ | `Merge/` |
| S2 planner vs git | ✅ Go | `docs/spikes/S2-planner-oracle.md` |
| S4 atomic save | ✅ Linux; Windows/macOS/WSL pending | `docs/spikes/S4-atomic-save.md` |
| S1 three-pane editor latency | ⚠ Conditional go (Linux CPU raster); GPU re-measure pending | `docs/spikes/S1-S3-editor.md`, `spikes/S1.ThreePane/` |
| S3 editor accessibility | ❌ Kill criterion met → **O1 decided 2026-11-11: option A** (accessible conflict navigator; screen-reader editing of RESULT is a v1 limitation) | `docs/spikes/S1-S3-editor.md` |

What each component guarantees:

- **Git runner:** argv only, no shell; `GIT_LITERAL_PATHSPECS=1`; no prompts; read-only calls avoid `index.lock`.
- **Unmerged index reader, stage acquisition:** inputs come from index stages in working-tree form (`cat-file --filters`), never from markers.
- **Text codec:** byte-exact round trip; lines split on LF only, like xdiff; no lossy decoding.
- **Line diffs:** the plan uses git's own Histogram diffs (S2); the in-process diffs are display-only.

`spikes/` holds throwaway measurement harnesses. They are not part of the solution, so CI doesn't build them. Run them from their own folders.

## Build & test

Requires the .NET 10 SDK and git ≥ 2.38 on `PATH`.

```bash
dotnet test CthulhuResolve.slnx
```

The tests build real repositories in temp folders through an isolated git environment. That means no user or system config, a fixed author and fixed dates, so your aliases, hooks, signing and `autocrlf` cannot change results.

## Extracting to its own repository (spec D6)

This folder shares nothing with the Rust workspace. To move it, with its history, into `cthulhu-resolve`:

```bash
git subtree split --prefix=resolver -b cthulhu-resolve
git push <new-remote> cthulhu-resolve:main
```
