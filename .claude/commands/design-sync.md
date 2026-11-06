---
description: Rebuild the GitCthulhu Design System artifact from the code and republish it
---

Rebuild the design system from `gui_app` and publish it to the Design System artifact
https://claude.ai/artifact/HzkiWqdCiVd8tvx24gJ72m. The artifact is marked `"editing": "code"`,
so its page is read-only and every sync replaces its files.

Sources: the authored files under `design/system/` (brand book, sections, usage notes, component
cards) and the code (`gui_app/src/theme.rs` colours, `commit_graph.rs` lanes, `icons.rs` font and
codepoints, the marks under `app/assets`). `gui_app/src/design_export.rs` is the exporter.

Steps, in order:

1. Build the files:
   ```bash
   cargo run -p design_sync
   ```
   It writes `target/design-system/project/` and prints what changed. Stop and tell the user if it
   fails (an `icons.md` row out of step with the `Icon` enum is the usual cause) or if it reports a
   colour **added** — that colour needs a usage note in `design/system/tokens.json` first.
2. Upload the three marks under `target/design-system/project/assets/Logos/` as assets of the
   artifact (Artifact `publish` with `asset: true`, `file_paths` for all three in one call). Note
   each file's returned id.
3. `read` the artifact's `project/design-system.json`. Take `createdOnFiles` (or `convertedFrom`)
   from it. Open `target/design-system/project/design-system.json`, add that key, and set each
   `assetGroups.Logos.files.<name>.blob` to the id from step 2. Keep everything else the exporter
   wrote, including `"editing": "code"` and `lastChange`.
4. Publish every file under `target/design-system/project/` in ONE call: `url` = the artifact,
   `root` = `target/design-system`, `file_path` = the absolute path of
   `target/design-system/project/design-system.json`, `files` = every other file by its
   `project/…` path (the three image files are uploads already recorded in the index: do not send
   them as files; `assets/Logos/README.md` is a file). Send `tokens.css` too: the page takes over a
   synced system's stylesheet from its first-line comment.
5. Say in two sentences what changed (the exporter's report) and give the link.

Never edit the artifact's files by hand: change `design/system/` or the code and run this again.
