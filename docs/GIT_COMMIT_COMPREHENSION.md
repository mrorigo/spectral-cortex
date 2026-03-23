# Git Commit Comprehension Plan

Last updated: 2026-02-14

## Why This Exists

Today, ingest creates exactly one `ConversationTurn` per git commit message after line-level filtering. This works for single-purpose commits, but it blurs multi-change commits into one embedding. The result is lower graph coherence and noisier retrieval.

This document proposes a resilient, efficient, and useful way to split one commit message into multiple semantic notes when appropriate.

## Current Behavior (Baseline)

The current ingest path is:

1. `collect_commits()` reads commits from `git2`.
2. `apply_git_line_filters()` removes configured noise lines.
3. One `ConversationTurn` is created per commit with:
   - `content = filtered commit message`
   - shared `commit_id` and `timestamp`
4. `ingest_turns_batch()` embeds each turn as one `SMGNote`.

Implication: if a message includes multiple changes, they are fused into one vector.

## Goals

1. Split multi-change commit messages into multiple turns when confidence is high.
2. Keep single-change commits as one turn.
3. Preserve strict traceability back to the original commit SHA and timestamp.
4. Keep ingestion fast and deterministic.
5. Fail safely: if parsing is uncertain, emit one turn (current behavior).

## Non-Goals

1. We are not parsing diffs in this phase.
2. We are not attempting perfect natural-language understanding.
3. We are not changing spectral/query math in this phase.

## Design Principles

1. Conservative by default: prefer false negatives over false positives.
2. Deterministic parsing: same input always yields same split.
3. Layered fallback: structured parsing first, heuristic parsing second, single-note fallback last.
4. Implementation-first rollout: ship `auto` as the default behavior.

## Proposed Architecture

Add a commit comprehension stage between filtering and turn construction.

Pipeline:

1. Raw commit message.
2. Existing `apply_git_line_filters()`.
3. `split_commit_message()` returns one or more `CommitSegment`.
4. Convert each segment to a `ConversationTurn`.
5. Batch embed as today.

### New Internal Types

Use small internal structs in `crates/spectral-cortex-cli/src/main.rs` first (can move later):

```rust
struct CommitSegment {
    header: String,
    details: Vec<String>,
    confidence: f32,
    parse_mode: ParseMode,
}

enum ParseMode {
    ConventionalHeader,
    BulletGrouped,
    ParagraphFallback,
}
```

## Splitting Strategy

Run the following stages in order.

### Stage 1: Conventional Header Detection

Detect lines that look like commit headers, for example:

- `refactor: use dependency injection`
- `fix(auth): add failure logs`
- `feat!: remove legacy cache`

Regex (example): `^(feat|fix|refactor|perf|docs|test|build|ci|chore|revert)(\\([^\\)]+\\))?(!)?:\\s+.+$`

Behavior:

1. Each detected header starts a new segment.
2. Following non-header lines attach to the current segment until the next header.
3. Bullet lines (`- ...`, `* ...`) are segment details.

### Stage 2: Bullet Grouping Heuristics

If Stage 1 found no headers, treat top-level bullets as candidate sub-changes when both are true:

1. There are at least 2 substantial bullets.
2. Bullet text similarity is low enough to suggest separate topics.

If confidence is low, do not split.

### Stage 3: Paragraph Fallback

If text contains multiple separated paragraphs and no reliable markers, optionally split by paragraph only when paragraph lengths and lexical distance suggest distinct changes. Otherwise keep one segment.

### Stage 4: Safety Fallback

If parser confidence is below threshold, emit one segment equal to current behavior.

## Turn Construction Rules

For each segment from a commit:

1. Create one `ConversationTurn`.
2. Preserve original `commit_id` and `timestamp` for every segment turn.
3. Keep deterministic turn ordering by segment index.
4. Build `content` as:

```text
<header>
<detail line 1>
<detail line 2>
```

5. Keep `topic = "git"` and `speaker = author_name` unchanged.

## CLI and Configuration Plan

Add new ingest/update flags:

1. `--git-commit-split-mode <off|auto|strict>`  
   - `off`: current one-note behavior  
   - `auto`: split only on high confidence (recommended default)  
   - `strict`: aggressively split on detected boundaries
2. `--git-commit-split-max-segments <n>` default `6`
3. `--git-commit-split-min-confidence <0..1>` default `0.75`

Rollout:

1. Phase 1 default `auto`.
2. Keep `off` and `strict` for debugging and tuning.

## Observability and Diagnostics

Extend ingest summary with split stats:

1. commits seen
2. commits split
3. total segments emitted
4. average segments per split commit
5. fallback-to-single count
6. parser mode distribution

Optional debug output:

- `--git-commit-split-debug-json <path>` to write per-commit parse decisions for tuning.

## Incremental and Compatibility Considerations

Current incremental mode skips commits already present by `commit_id`. With splitting enabled, this means existing one-note historical commits will not be retro-split during normal incremental updates.

Plan:

1. Document this behavior clearly.
2. No special rebuild flag is required; ingest always rebuilds spectral structures.
3. Keep incremental semantics unchanged for performance and simplicity.

## Performance Considerations

1. Parsing is linear in message length and negligible vs embedding cost.
2. Segment count cap prevents pathological note explosion.
3. Conservative split threshold limits extra embeddings.
4. No additional network/model dependencies.

## Testing Plan

Add unit tests for parser behavior:

1. single conventional commit -> one segment
2. two conventional headers -> two segments
3. header with bullets -> one segment with details
4. mixed bad formatting -> stable fallback
5. empty/noise-only message -> skipped as today
6. max-segment cap enforcement

Add integration tests:

1. ingest fixture with mixed commits and verify emitted note count
2. verify all split notes share original `commit_id`
3. verify deterministic output ordering
4. verify incremental skip behavior remains unchanged

## Phased Implementation Plan

1. Phase 1: parser + feature flags + ingest stats + tests (default `auto`)
2. Phase 2: tuning on real repos using debug JSON and retrieval checks
3. Phase 3: update README and MCP docs
4. Phase 4: optional metadata enrichment for explainability

## Success Criteria

1. Better retrieval precision for multi-topic commit history queries.
2. No regression in single-topic commit retrieval.
3. Ingest runtime overhead stays small relative to embedding.
4. Deterministic and stable behavior across repeated runs.
