# ADR 0006: Immutable merge plan; the result is an anchored document

Status: Accepted 2026-10-27

## Context

diff3 is not stable under small perturbations (Khanna, Kunal & Pierce, 2007). If region identity were recomputed from the edited result, manual edits could re-identify or merge hunks. Bulk actions would then risk overwriting manual work.

## Decision

The merge plan is computed once per file from the immutable stages (base, left, right) and frozen. Each plan region has a stable id. The editable result is a rope holding two anchors per region: the start anchor sticks left, the end anchor sticks right. Region state comes from the actions applied plus a hash of the expected text, which separates `Resolved(choice)` from `Manual`.

Rules:

- An edit that straddles regions marks every touched region `Manual`, unacknowledged.
- Bulk actions skip `Manual` regions.
- Undo is a log of (text change, region transition) pairs.
- Whitespace-ignore and other display modes never alter the plan.

## Why

Identity derived from inputs is deterministic and needs no re-diffing. The anchor map is monotone, so region order is invariant. Undo restores text and state together.

## Consequences

- The "what changed" display (result versus each input) is a separate per-region computation.
- A planner version change invalidates stored region mappings. The session format carries a `planDigest` so it can detect this.
