# Correct filtered retrieval indexing

## Objective

Make filtered retrieval use cluster labels and note metadata from the same global note ordering.

## Scope

- Fix `retrieve_candidates_filtered`.
- Reject or skip missing note IDs without panicking.
- Preserve deterministic ordering.

## Implementation

1. Build a global sorted `note_ids` vector matching `cluster_labels`.
2. Build a note-ID-to-index map.
3. Translate each filtered note ID through that map before reading its cluster label.
4. Decide and document the malformed-data behavior: return an error for an unknown ID.
5. Add tests with a non-contiguous filtered subset and cluster labels that distinguish every note.
6. Add a test with an unknown filtered ID and assert the documented error.

## Acceptance criteria

- Filtered retrieval boosts only the correct clusters.
- Unknown IDs never cause an index panic.
- Results match unfiltered retrieval restricted to the selected note IDs.
