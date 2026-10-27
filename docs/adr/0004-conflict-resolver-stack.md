# ADR 0004: Conflict resolver is .NET + Avalonia + AvaloniaEdit, editor behind a port

Status: Accepted 2026-10-27. Spikes S1 (latency) and S3 (accessibility) still decide between AvaloniaEdit and the Monaco fallback.

## Context

GitCthulhu needs a trustworthy three-way conflict resolver. The iced 0.13 `text_editor` has no gutter, line backgrounds or undo, so it cannot host one without first building an editor. Neither Monaco nor `@codemirror/merge` ships a three-way merge editor; both offer two-way diff only. VS Code's merge editor lives in its workbench, not in the `monaco-editor` package. The owner's core strength is .NET. Spec: `docs/superpowers/specs/2026-10-27-conflict-resolver-design.md` §6.1.

## Decision

A separate .NET 10 application, `cthulhu-resolve`:

- **`Resolve.Core`** is headless and owns Git, diff, merge plan, result document, sessions and validation.
- **Avalonia 12 + AvaloniaEdit** is the UI.
- **Alignment uses the band model** (connector bands and a monotone piecewise-linear scroll map), not spacer lines.
- **The editor sits behind `IEditorSurface`.** If S1 (latency at 100k lines) or S3 (accessibility peers) fails, Monaco in `Avalonia.Controls.WebView` replaces the editor behind the same port.

GitCthulhu starts the resolver as a process through its CLI.

## Why

The band model needs line↔y mapping, background renderers and margins, and AvaloniaEdit has all three. That removes Monaco's main advantage (view zones). The core stays in the owner's strongest stack and is testable without a GUI. The process boundary keeps the Rust window and the .NET resolver independently releasable.

## Consequences

- We own the three-pane orchestration and an accessibility peer for AvaloniaEdit.
- Two toolchains in the wider product.
- A fallback path exists that does not touch the core.
