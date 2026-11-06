# design/

`system/` is the authored half of the GitCthulhu Design System artifact
(https://claude.ai/artifact/HzkiWqdCiVd8tvx24gJ72m): the brand book (`README.md`), the Layout and
Icons sections, `tokens.json` with its usage notes, and the component cards under `components/`.

The other half comes from the code at sync time — every colour in `gui_app/src/theme.rs`, the
graph lanes, the Codicons font and codepoints, the marks under `app/assets` — stamped by
`gui_app/src/design_export.rs` and run by:

```bash
cargo run -p design_sync
```

That writes `target/design-system/project/` (never committed). `/design-sync` in Claude Code runs
it and republishes the artifact. The artifact's page is read-only (`"editing": "code"`): to change
the system, edit these files or the code and sync again.

Rules the exporter enforces: colour values in `tokens.json` are overwritten from `DesignTokens`
(the notes stay); a colour the code adds is appended with an empty note and reported; every
`Icon` variant must have a row in `icons.md` with its codepoint, and no row may be stale.
