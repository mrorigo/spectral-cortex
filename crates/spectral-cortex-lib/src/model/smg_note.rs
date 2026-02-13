// Plain Vec<f32> for the embedding. No external serialization needed.

use serde::{Deserialize, Serialize};

/// Node stored in the Spectral Memory Graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SMGNote {
    pub note_id: u32,
    pub raw_content: String,
    pub context: String,
    pub embedding: Vec<f32>,
    /// Precomputed L2 norm of the embedding for fast cosine similarity computation.
    /// This is computed once during ingestion and reused during queries.
    pub norm: f32,
    pub source_turn_ids: Vec<u64>,
    /// Optional list of source commit ids (SHA hex strings) associated with this note.
    /// Kept in parallel with `source_turn_ids` to allow direct lookup of VCS commits
    /// when a query returns a note/turn.
    /// Each entry is optional to represent synthetic/non-git turns.
    pub source_commit_ids: Vec<Option<String>>,
    /// Parallel vector of source timestamps (unix epoch seconds) corresponding to
    /// `source_turn_ids`. This field is used for temporal re-ranking and must be
    /// populated during ingest/update.
    pub source_timestamps: Vec<u64>,
    // Placeholder for future spectral coordinates – not used in the stub.
    pub spectral_coords: Option<Vec<f32>>, // not persisted directly
    pub related_note_ids: Vec<u32>,
}

impl SMGNote {
    /// Update the note with a new turn, performing a weighted average of embeddings.
    pub fn update_with_turn(
        &mut self,
        turn: &crate::model::conversation_turn::ConversationTurn,
        new_emb: &[f32],
    ) {
        // Concatenate raw content and context.
        self.raw_content.push_str(" | ");
        self.raw_content.push_str(&turn.content);
        self.context.push(' ');
        self.context.push_str(&turn.clean_context());

        // Weighted average of embeddings (element‑wise).
        let n = self.source_turn_ids.len() as f32;
        for (e, ne) in self.embedding.iter_mut().zip(new_emb.iter()) {
            *e = (*e * n + *ne) / (n + 1.0);
        }
        // Recompute norm after embedding update.
        self.norm = self.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Record the source turn id.
        self.source_turn_ids.push(turn.turn_id);

        // Record the optional commit id in parallel with the turn id.
        // If the ConversationTurn has no commit id this pushes `None`.
        self.source_commit_ids.push(turn.commit_id.clone());

        // Record the timestamp (unix epoch seconds) in parallel with the turn id.
        self.source_timestamps.push(turn.timestamp);
    }
}
