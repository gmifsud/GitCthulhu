### Task 5: Unified hunk visual refresh

**Files:**
- Modify: `gui_app/src/view/patch.rs`
- Modify: `gui_app/src/theme.rs` (`DIFF_TINT` or dedicated stronger constants)

**Interfaces:**
- Consumes: `DiffLineKind`, `DesignTokens::{added_bg, removed_bg, added, removed}`
- Produces: denser `diff_line` rows with gutter `+`/`-`/` `

- [ ] **Step 1: Raise tint**

In `theme.rs`:

```rust
const DIFF_TINT: f32 = 0.22; // was 0.12
```

- [ ] **Step 2: Gutter marker column**

In `diff_line`:

```rust
let mark = match kind {
    DiffLineKind::Added => "+",
    DiffLineKind::Removed => "-",
    DiffLineKind::Context => " ",
};
// row: [old#][new#][mark 12px][text]
```

Style mark with `added` / `removed` / `text_muted`.

- [ ] **Step 3: Tighten padding** — reduce vertical padding on line rows (e.g. `[0, 4]` → `[0, 2]`).

- [ ] **Step 4: Manual compare to GitKraken reference image (unified approximation)**

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: denser unified hunk gutters and tints

EOF
)"
```

---


