# Make metadata filters exclude nonmatching notes

## Objective

Ensure `file_filter` and `symbol_filter` are true filters rather than zero-score penalties.

## Scope

- Both candidate retrieval methods.
- Candidate expansion and temporal reranking.

## Implementation

1. Reject a note from the scoring iterator when it fails a requested filter.
2. Apply filters before selecting the top `candidate_note_k` notes.
3. Preserve notes with missing metadata only when no corresponding filter is supplied.
4. Add tests where matching notes are fewer than `candidate_note_k`.
5. Add combined file-and-symbol filter tests.

## Acceptance criteria

- A result never violates an active metadata filter.
- The result count is at most the number of matching source turns.
- Filtered queries return no candidates when no note matches.
