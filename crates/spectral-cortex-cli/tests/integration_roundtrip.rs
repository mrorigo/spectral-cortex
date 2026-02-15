use anyhow::Result;
use spectral_cortex::embed;
use spectral_cortex::{load_smg_json, save_smg_json, ConversationTurn, SpectralMemoryGraph};
use std::time::{SystemTime, UNIX_EPOCH};

/// Integration test: ingest -> build -> save -> load -> query roundtrip.
///
/// This test uses the deterministic fake embedder (the crate's default when the
/// `real-embed` feature is not enabled) so it is stable and suitable for CI.
///
/// The test:
/// 1. Constructs a small set of synthetic `ConversationTurn` entries.
/// 2. Ingests them into a `SpectralMemoryGraph`.
/// 3. Builds spectral structures.
/// 4. Persists the SMG to a temporary JSON file.
/// 5. Loads the SMG back from disk.
/// 6. Runs `retrieve` on both the original and reloaded SMG and asserts results.
///
/// The test returns `Result<()>` so the `?` operator can be used for brevity.
#[test]
fn integration_roundtrip() -> Result<()> {
    // 1) Prepare synthetic conversation turns (small, varied set).
    let samples = [
        "fix bug in parser",
        "add new feature for export",
        "refactor storage layer",
        "update documentation and README",
        "write unit tests for spectral utils",
        "optimize query performance",
        "cleanup unused imports",
        "improve logging and telemetry",
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

    // 2) Initialize embed pool and create a fresh SMG.
    embed::init(1, 0)?;
    let mut smg = SpectralMemoryGraph::new()?;
    for t in &turns {
        smg.ingest_turn(t)?;
    }

    // 3) Build spectral structures (embeddings, clustering, centroids, links).
    smg.build_spectral_structure(None)?;

    // 4) Persist to a temporary file in the OS temp directory.
    let mut out_path = std::env::temp_dir();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    out_path.push(format!("spectral_cortex_smg_test_{}.json", stamp));
    save_smg_json(&smg, &out_path)?;

    // 5) Load the persisted SMG.
    let loaded = load_smg_json(&out_path)?;

    // Sanity checks: same note count and presence of cluster labels/centroids in the persisted file.
    assert_eq!(smg.notes.len(), loaded.notes.len());
    // The original SMG ought to have cluster labels after a build; persisted file should restore them.
    assert!(
        smg.cluster_labels.is_some() && loaded.cluster_labels.is_some(),
        "expected cluster labels to be present in both original and loaded SMG"
    );

    // 6) Run retrieval on both graphs for a representative query and assert non-empty results.
    let query = "fix bug";
    let res_original = smg.retrieve(query, 5)?;
    let res_loaded = loaded.retrieve(query, 5)?;

    assert!(
        !res_original.is_empty(),
        "original SMG returned no retrieval results"
    );
    assert!(
        !res_loaded.is_empty(),
        "loaded SMG returned no retrieval results"
    );

    // Ensure some overlap between original and loaded results (sanity that persistence preserved content).
    let common = res_original
        .into_iter()
        .filter(|id| res_loaded.contains(id))
        .collect::<Vec<_>>();
    assert!(
        !common.is_empty(),
        "no common retrieval ids between original and loaded SMG"
    );

    // Cleanup the temporary file; ignore errors during cleanup.
    let _ = std::fs::remove_file(&out_path);

    Ok(())
}
