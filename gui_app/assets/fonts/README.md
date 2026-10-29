# Bundled fonts

| File | Source | Version | Licence |
|---|---|---|---|
| `codicon.ttf` | [`@vscode/codicons`](https://github.com/microsoft/vscode-codicons) `dist/codicon.ttf` | 0.0.45 | CC BY 4.0 (`LICENSE-codicons.txt`), © Microsoft Corporation |

The icon font is compiled into the binary (`include_bytes!` in `gui_app/src/icons.rs`), so icons do not depend on the fonts installed on the machine.

SHA-256 of `codicon.ttf`: `2bb558cb693451e73c28c33fe64aa89bc19b1a4b70f95948322c243f93476920`.

To update, download the new release's `codicon.ttf`, check the codepoints in its `dist/codicon.csv` against `Icon::codepoint`, and replace the file and the hash.
