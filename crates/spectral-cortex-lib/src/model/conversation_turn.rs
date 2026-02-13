// Timestamp as seconds since UNIX epoch (u64) to avoid external chrono crate.
// Serde derives are added to enable JSON/Serde-based persistence of ConversationTurn.
// This file intentionally avoids adding external date/time crates to keep the surface small.
use serde::{Deserialize, Serialize};

/// Represents a single turn in a conversation or a git commit record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub turn_id: u64,
    pub speaker: String,
    pub content: String,
    pub topic: String,
    pub entities: Vec<String>,
    /// Optional commit id (SHA) for turns originating from a VCS commit.
    /// Stored as `Option<String>` so synthetic or non-git turns can omit it.
    pub commit_id: Option<String>,
    pub timestamp: u64,
}

impl ConversationTurn {
    /// Returns a whitespace‑collapsed version of `content`.
    pub fn clean_context(&self) -> String {
        self.content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}
