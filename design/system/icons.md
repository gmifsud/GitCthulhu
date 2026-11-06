# Icons

All 44 icons come from Codicons 0.0.45 (`fonts/codicon.ttf`, © Microsoft, CC BY 4.0), compiled into the binary. Set them as text in the `icon` family at a `glyph` style; colour them with the text token of the row they sit in. A test checks that every codepoint below is in the font's cmap, so an icon never draws as a box.

| Icon | Codepoint | Where |
|---|---|---|
| add | U+EA60 | The tab-picker button; new branch, stash, worktree |
| arrow-small-down | U+EA9D | Incoming count (`↓`), in `incoming` |
| arrow-small-up | U+EAA0 | Outgoing count (`↑`), in `outgoing` |
| branch | U+EA68 | The branch chip; Head and Local ref pills |
| check | U+EAB2 | A succeeded job in the status bar; stage |
| check-all | U+EBB1 | Stage all |
| chevron-down | U+EAB4 | An open fold; the chip while its popup is open |
| chevron-left | U+EAB5 | Back |
| chevron-right | U+EAB6 | A closed fold; the Shell prompt header mark |
| circle-filled | U+EA71 | A plain state dot |
| clear-all | U+EABF | Clear the filter |
| close | U+EA76 | Close a tab; a failed job; dismiss an error |
| cloud | U+EBAA | Remote ref pills |
| commit | U+EAFC | Commit |
| compare | U+EAFD | Compare two refs |
| discard | U+EAE2 | Discard changes |
| edit | U+EA73 | Rename, edit message |
| fetch | U+EC1D | Fetch |
| file | U+EA7B | A file row |
| folder | U+EA83 | A closed folder row |
| folder-opened | U+EAF7 | An open folder row |
| list-flat | U+EB84 | Flat file list |
| list-tree | U+EB86 | Tree file list |
| merge | U+EAFE | Merge |
| pin | U+EBA0 | Pin the Repositories column |
| pinned | U+EBB2 | The column is pinned |
| pull | U+EB40 | Pull |
| push | U+EB41 | Push |
| refresh | U+EB37 | Refresh; a running job in the status bar |
| remove | U+EB3B | Unstage |
| repo | U+EA62 | A repository row |
| search | U+EA6D | The filter bar's search field |
| stop | U+EAD7 | Stop a job; a stopped job |
| terminal | U+EA85 | Show the terminal |
| sparkle | U+EC10 | The agent |
| stash-apply | U+EC27 | Apply a stash |
| stash-pop | U+EC28 | Pop a stash |
| stash | U+EC26 | Stash |
| split-view | U+EB56 | Side-by-side diff |
| switch-branch | U+EBCB | Switch branch |
| tag | U+EA66 | Tag ref pills; new tag |
| target | U+EBF8 | Jump to HEAD |
| trash | U+EA81 | Delete a branch or stash |
| worktree | U+EA63 | Worktree ref pills; add a worktree |

Fold chevrons: `chevron-right` closed, `chevron-down` open. Ref pill marks: Head and Local `branch`, Worktree `worktree`, Remote `cloud`, Tag `tag`.
