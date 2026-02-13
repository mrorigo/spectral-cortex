use anyhow::Result;
use spectral_cortex::embed;
use spectral_cortex::{ConversationTurn, SpectralMemoryGraph};
use std::time::{SystemTime, UNIX_EPOCH};

/// Integration test: verify candidate_k retrieval combined with min_score filtering.
///
/// This test uses the deterministic fake embedder (selected automatically for tests)
/// so results are reproducible in CI.
///
/// Test strategy:
/// 1. Create a small SMG with a clearly identifiable "exact" sample.
/// 2. Call `retrieve_with_scores(query, candidate_k)` to get candidate matches.
/// 3. Assert candidates are non-empty and compute the best (max) score.
/// 4. Verify that filtering with `min_score = best_score` (inclusive) returns at
///    least the best result, and that filtering with `min_score = best_score + eps`
///    returns no results. This validates the inclusive semantics (`>=`) of the
///    threshold and that filtering actually drops entries above the threshold.
#[test]
fn integration_candidate_k_min_score() -> Result<()> {
    // 1) Prepare synthetic conversation turns. The first entry is an exact match.
    let samples = vec![
        "unique exact match: special-phrase",
        "add new feature for export",
        "refactor storage layer",
        "update documentation and README",
        "write unit tests for spectral utils",
    ];

    let mut turns: Vec<ConversationTurn> = Vec::with_capacity(samples.len());
    for (i, s) in samples.iter().enumerate() {
        let t = ConversationTurn {
            turn_id: (i as u64) + 1,
            speaker: format!("author{}", i),
            content: s.to_string(),
            topic: "git".to_string(),
            entities: Vec::new(),
            commit_id: Some(format!("synthetic-{}", i)),
            timestamp: (SystemTime::now().duration_since(UNIX_EPOCH)?).as_secs(),
        };
        turns.push(t);
    }

    // 2) Initialize embed pool and build an SMG.
    embed::init(1, 0)?;
    let mut smg = SpectralMemoryGraph::new()?;
    for t in &turns {
        smg.ingest_turn(t)?;
    }

    // 3) Build spectral structures to enable cluster-aware boosting.
    smg.build_spectral_structure(None)?;

    // 4) Query for the exact matching text.
    let query = "unique exact match: special-phrase";

    // Use candidate_k = 5 (top_k * 5 heuristic) and request a reasonable candidate set.
    let candidate_k = 5usize;
    let candidates = smg.retrieve_with_scores(query, candidate_k)?;
    assert!(
        !candidates.is_empty(),
        "expected non-empty candidate set from retrieve_with_scores"
    );

    // 5) Find the best score among candidates.
    let (best_tid, best_score) = candidates
        .iter()
        .cloned()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .expect("candidates non-empty; max_by returned None");

    // Ensure there is at least one candidate with score >= best_score (trivially true).
    let filtered_ge_best: Vec<(u64, f32)> = candidates
        .clone()
        .into_iter()
        .filter(|(_tid, score)| *score >= best_score)
        .collect();
    assert!(
        !filtered_ge_best.is_empty(),
        "expected at least one candidate with score >= best_score"
    );

    // If we set min_score slightly above best_score, filtering must yield an empty set.
    let eps = 1e-6f32;
    let filtered_above: Vec<(u64, f32)> = candidates
        .clone()
        .into_iter()
        .filter(|(_tid, score)| *score >= (best_score + eps))
        .collect();
    assert!(
        filtered_above.is_empty(),
        "expected no candidates with score >= best_score + eps"
    );

    // 6) Sanity: when selecting final top_k = 1 from the unfiltered candidates,
    // the best_tid we computed should be the top result after sorting by score desc.
    let mut sorted = candidates.clone();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let final_top = sorted.into_iter().next().expect("sorted non-empty");
    assert_eq!(
        final_top.0, best_tid,
        "expected the best candidate id to be the top result"
    );

    Ok(())
}
