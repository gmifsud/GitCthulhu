# ADR 0005: The git CLI is authoritative for conflict inputs, staging and continuation

Status: Proposed

## Context

Conflict inputs must match what Git recorded:

- the unmerged index stages and modes
- the working-tree form after filters, `eol` and `working-tree-encoding`
- the operation state (ort, both rebase backends, `AUTO_MERGE`)

LibGit2Sharp 0.32 has no `git_merge_file` binding, and libgit2 diverges from Git in filter and attribute handling. Parsing conflict markers cannot reconstruct the base.

## Decision

All repository reads and writes in the resolver go through the `git` executable via a typed runner:

- arguments are passed as an argv vector; no shell
- `-z` output wherever a command supports it
- `--` before paths, with `:(literal)` pathspecs
- `GIT_TERMINAL_PROMPT=0`
- `GIT_OPTIONAL_LOCKS=0` for status reads

Inputs are acquired from stages with `checkout-index --temp --stage=all` or `cat-file --batch --filters`, never from markers. The minimum Git version is 2.38. `git merge-file --zdiff3` is the test oracle for the planner.

## Why

The CLI gives Git's own semantics, including the user's config, `safe.directory`, filters and the latest merge machinery, with no licence or binding gap. Process cost is negligible next to user think-time.

## Consequences

- A version probe gates features.
- Paths and output are handled as bytes.
- Configured clean/smudge filters run during staging, and the UI discloses this.
