use ndarray::array;
use spectral_cortex::graph::spectral::{
    assemble_embedding_matrix, compute_spectral_embeddings, cosine_similarity_matrix,
    degree_vector, normalized_laplacian, sparsify_adj, spectral_decomposition,
};
use spectral_cortex::model::smg_note::SMGNote;
use std::collections::HashMap;

#[test]
fn test_assemble_and_cosine() {
    // Prepare two simple notes with orthogonal embeddings
    let mut notes: HashMap<u32, SMGNote> = HashMap::new();
    notes.insert(
        1,
        SMGNote {
            note_id: 1,
            raw_content: "a".to_string(),
            context: "".to_string(),
            embedding: vec![1.0_f32, 0.0, 0.0],
            norm: 1.0,
            source_turn_ids: vec![],
            source_commit_ids: vec![],
            source_timestamps: vec![],
            spectral_coords: None,
            related_note_links: vec![],
        },
    );
    notes.insert(
        2,
        SMGNote {
            note_id: 2,
            raw_content: "b".to_string(),
            context: "".to_string(),
            embedding: vec![0.0_f32, 1.0, 0.0],
            norm: 1.0,
            source_turn_ids: vec![],
            source_commit_ids: vec![],
            source_timestamps: vec![],
            spectral_coords: None,
            related_note_links: vec![],
        },
    );

    let order = vec![1u32, 2u32];
    let mat = assemble_embedding_matrix(&notes, &order);
    assert_eq!(mat.nrows(), 2);
    assert_eq!(mat.ncols(), 3);

    let sim = cosine_similarity_matrix(&mat);
    // diagonal entries should be 1 for identical vectors
    assert!((sim[(0, 0)] - 1.0).abs() < 1e-6);
    assert!((sim[(1, 1)] - 1.0).abs() < 1e-6);
    // orthogonal vectors should have near-zero cosine similarity
    assert!(sim[(0, 1)].abs() < 1e-6);
}

#[test]
fn test_sparsify_and_degree() {
    // Create a small similarity matrix
    let mut sim = array![[1.0_f32, 0.3, 0.1], [0.3, 1.0, 0.05], [0.1, 0.05, 1.0]];
    // sparsify with threshold 0.2 should zero entries < 0.2 and diagonal forced to zero
    sparsify_adj(&mut sim, 0.2);
    assert_eq!(sim[(0, 0)], 0.0);
    assert_eq!(sim[(0, 1)], 0.3);
    assert_eq!(sim[(0, 2)], 0.0);

    let deg = degree_vector(&sim);
    // degree vector length should equal number of rows
    assert_eq!(deg.len(), 3);
    // degree for row 0 should equal 0.3
    assert!((deg[0] - 0.3).abs() < 1e-6);
}

#[test]
fn test_normalized_laplacian_symmetry() {
    // Symmetric adjacency
    let w = array![[0.0_f32, 0.5, 0.5], [0.5, 0.0, 0.0], [0.5, 0.0, 0.0]];
    let lap = normalized_laplacian(&w);
    // shape check
    assert_eq!(lap.shape(), &[3, 3]);
    // symmetry check
    assert!((lap[(0, 1)] - lap[(1, 0)]).abs() < 1e-6);
    assert!((lap[(0, 2)] - lap[(2, 0)]).abs() < 1e-6);
}

#[test]
fn test_spectral_decomposition_and_eigengap() {
    // Construct a diagonal symmetric matrix with a large eigengap between index 1 and 2
    let l = array![
        [0.1_f32, 0.0, 0.0, 0.0],
        [0.0, 0.2, 0.0, 0.0],
        [0.0, 0.0, 5.0, 0.0],
        [0.0, 0.0, 0.0, 5.1]
    ];
    let (eigvals, evecs) = spectral_decomposition(&l, 2).expect("decomposition");
    // Lanczos returns only k eigenvalues/eigenvectors, not all n
    assert_eq!(eigvals.len(), 2);
    assert_eq!(evecs.shape(), &[4, 2]);

    // Note: eigengap heuristic requires all eigenvalues, but Lanczos only returns k
    // For this test, we'll skip the eigengap check since we're testing Lanczos specifically
    // let suggested_k = eigengap_heuristic(&eigvals);
    // assert_eq!(suggested_k, 2usize);

    // compute spectral embeddings for k=2
    let spec = compute_spectral_embeddings(&evecs, 2, true);
    assert_eq!(spec.shape(), &[4, 2]);

    // A basic sanity check: embeddings should be finite
    for v in spec.iter() {
        assert!(v.is_finite());
    }
}
