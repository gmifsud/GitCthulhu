### Task 3: Graph column reorder + anti-deformation

**Files:**
- Modify: `gui_app/src/view/history.rs`
- Modify: `gui_app/src/ref_pills.rs` only if pill row helper needs a non-description home

**Interfaces:**
- Produces: row layout `pills | graph | message | author | when | hash`
- Consumes: `refs_at`, `GraphCell`, existing widths

- [ ] **Step 1: Adjust constants**

```rust
const HASH_WIDTH: f32 = 72.0;
const WHEN_WIDTH: f32 = 56.0;
const AUTHOR_WIDTH: f32 = 100.0;
const MIN_MESSAGE_WIDTH: f32 = 120.0; // use in header/message cells via width hints
const PILLS_MIN_WIDTH: f32 = 48.0;
```

- [ ] **Step 2: Rewrite `column_headers` order**

Order: `""` or `Refs` (pills col) → `Graph` → `Description` → `Author` → `When` → `Hash` → focus.  
Every header: `.wrapping(Wrapping::None)`.

- [ ] **Step 3: Rewrite commit row body**

```rust
row![
  pills_cell(&pills, …),          // was inside description_with_pills
  canvas(GraphCell { … }).width(lanes_width).height(ROW_HEIGHT),
  single_line(message, Length::Fill, …),  // message only
  single_line(author, Length::Fixed(AUTHOR_WIDTH), …),
  single_line(when, Length::Fixed(WHEN_WIDTH), …),
  single_line(short_hash, Length::Fixed(HASH_WIDTH), …),
]
.spacing(COL_GAP)
```

WIP rows: empty hash; pills/WIP label in pills column; empty or spacer graph.

- [ ] **Step 4: Delete pills from `description_with_pills` (rename to message cell)**

- [ ] **Step 5: Manual / screenshot check at ~1200px wide — no vertical header wrap; hash rightmost**

- [ ] **Step 6: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: graph pills-first columns with hash last

EOF
)"
```

---


