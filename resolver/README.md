# Cthulhu Resolve

Standalone three-way Git conflict resolver.

- **Spec:** `../docs/superpowers/specs/2026-10-27-conflict-resolver-design.md`
- **Decisions:** ADR 0004–0006 in `../docs/adr/`

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
| S1 three-pane editor latency | ⏳ next | — |
| S3 editor accessibility | ⏳ needs NVDA / VoiceOver on a real machine | — |

What each component guarantees:

- **Git runner:** argv only, no shell; `GIT_LITERAL_PATHSPECS=1`; no prompts; read-only calls avoid `index.lock`.
- **Unmerged index reader, stage acquisition:** inputs come from index stages in working-tree form (`cat-file --filters`), never from markers.
- **Text codec:** byte-exact round trip; lines split on LF only, like xdiff; no lossy decoding.
- **Line diffs:** the plan uses git's own Histogram diffs (S2); the in-process diffs are display-only.

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
