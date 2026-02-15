/*
spectral-memory-graph/rust-version/crates/spectral-cortex-lib/src/temporal.rs

Temporal re-ranking utilities: configuration, scoring modes and a single
entrypoint `re_rank_with_temporal` which enriches candidates with a
temporal score and combined final score and returns them sorted by final_score
descending.

Design notes:
- Uses UNIX epoch seconds (u64) for timestamps and for injectable `now` value.
- Missing timestamps are treated as very old (temporal_score = 0.0).
- Default configuration matches project defaults: enabled=true, weight=0.20,
  mode=Exponential, half_life_days=14.
- This module avoids adding new external dependencies beyond those already
  present in the crate (serde is used for configuration serde).
*/

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::f64::consts::LN_2;
use std::time::{SystemTime, UNIX_EPOCH};

// Import model types to make it straightforward to reference note/turn structures
// from tests or future extensions. These imports are not strictly required for
// the current implementation (which uses plain u64 timestamps), but they make
// extending temporal logic to use richer types simpler and clearer.

/// Default temporal weight: 20% influence from temporal signal.
pub const DEFAULT_TEMPORAL_WEIGHT: f32 = 0.20;
/// Default half-life in days (two weeks).
pub const DEFAULT_HALF_LIFE_DAYS: f32 = 14.0;

/// Convert days -> seconds (as u64)
fn days_to_seconds(days: f32) -> u64 {
    ((days * 24.0 * 60.0 * 60.0).round() as u64).max(1)
}

/// Temporal scoring modes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemporalMode {
    Exponential,
    LinearWindow,
    Step,
    Buckets,
}

/// Configuration for temporal re-ranking.
///
/// Fields are intentionally simple and documented so callers can serialize/deserialize
/// the effective configuration for diagnostics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalConfig {
    /// Whether temporal reranking is enabled. Default: true.
    pub enabled: bool,
    /// Weight of the temporal signal in [0.0, 1.0]. Default: DEFAULT_TEMPORAL_WEIGHT.
    pub weight: f32,
    /// Chosen temporal mode. Default: Exponential.
    pub mode: TemporalMode,
    /// Exponential half-life in seconds (if applicable).
    /// If `None` the default half-life of DEFAULT_HALF_LIFE_DAYS is used.
    pub half_life_seconds: Option<u64>,
    /// Window size in seconds for linear/step modes.
    pub window_seconds: Option<u64>,
    /// For step mode: magnitude of the boost (0..1). If None, defaults to 1.0.
    pub boost_magnitude: Option<f32>,
    /// Optional explicit bucket mapping (age_seconds -> score). If present, it is
    /// interpreted as a list of (max_age_seconds, score) sorted by ascending max_age_seconds.
    /// The first matching bucket is used. Scores should be in [0,1].
    pub buckets: Option<Vec<(u64, f32)>>,
    /// Optional override of "now" for deterministic tests (unix epoch seconds).
    pub now_seconds: Option<u64>,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        TemporalConfig {
            enabled: true,
            weight: DEFAULT_TEMPORAL_WEIGHT,
            mode: TemporalMode::Exponential,
            half_life_seconds: Some(days_to_seconds(DEFAULT_HALF_LIFE_DAYS)),
            window_seconds: None,
            boost_magnitude: None,
            buckets: None,
            now_seconds: None,
        }
    }
}

/// A retrieval candidate that the re-ranker will accept.
///
/// `timestamp` is optional and expressed as seconds since UNIX epoch (UTC).
#[derive(Clone, Debug)]
pub struct Candidate {
    pub turn_id: u64,
    pub note_id: u32,
    /// raw semantic score (e.g. cosine sim; expected in 0..1).
    pub raw_score: f32,
    /// Optional epoch seconds timestamp associated with the candidate.
    pub timestamp: Option<u64>,
}

/// Candidate with computed temporal and final scores produced by the re-ranker.
#[derive(Clone, Debug)]
pub struct CandidateWithScores {
    pub candidate: Candidate,
    /// Normalized temporal score (0..1).
    pub temporal_score: f32,
    /// Final combined score used for ranking and filtering.
    pub final_score: f32,
}

impl CandidateWithScores {
    pub fn turn_id(&self) -> u64 {
        self.candidate.turn_id
    }
}

/// Compute temporal score for a single candidate according to the configured mode.
///
/// - `now_seconds` must be >= the candidate timestamp when timestamp is present.
/// - Missing timestamp -> 0.0 (very old).
fn compute_temporal_score(
    candidate_ts: Option<u64>,
    now_seconds: u64,
    cfg: &TemporalConfig,
) -> f32 {
    // If disabled, temporal_score is zeroed by the caller logic, but keep function pure.
    let ts = match candidate_ts {
        Some(t) => t,
        None => return 0.0_f32,
    };

    // Guard: avoid underflow if now < ts (future-dated candidates). Treat age 0 in that case.
    let age_seconds = now_seconds.saturating_sub(ts);

    match cfg.mode {
        TemporalMode::Exponential => {
            let half_life = cfg
                .half_life_seconds
                .unwrap_or_else(|| days_to_seconds(DEFAULT_HALF_LIFE_DAYS))
                as f64;
            // Avoid division by zero.
            if half_life <= 0.0 {
                return 0.0;
            }
            let age = age_seconds as f64;
            let score = (-LN_2 * age / half_life).exp();
            // Clamp to [0,1]
            if score.is_nan() {
                0.0
            } else {
                score.clamp(0.0, 1.0) as f32
            }
        }
        TemporalMode::LinearWindow => {
            let window = cfg
                .window_seconds
                .unwrap_or_else(|| days_to_seconds(DEFAULT_HALF_LIFE_DAYS))
                as f64;
            if window <= 0.0 {
                return 0.0;
            }
            let age = age_seconds as f64;
            let val = 1.0 - (age / window);
            val.clamp(0.0, 1.0) as f32
        }
        TemporalMode::Step => {
            let window = cfg
                .window_seconds
                .unwrap_or_else(|| days_to_seconds(DEFAULT_HALF_LIFE_DAYS));
            let magnitude = cfg.boost_magnitude.unwrap_or(1.0).clamp(0.0, 1.0);
            if age_seconds <= window {
                magnitude
            } else {
                0.0
            }
        }
        TemporalMode::Buckets => {
            if let Some(ref buckets) = cfg.buckets {
                for (max_age, score) in buckets.iter() {
                    if age_seconds <= *max_age {
                        return score.clamp(0.0, 1.0);
                    }
                }
                0.0
            } else {
                // Fallback to exponential if no buckets are provided.
                let half_life = cfg
                    .half_life_seconds
                    .unwrap_or_else(|| days_to_seconds(DEFAULT_HALF_LIFE_DAYS))
                    as f64;
                if half_life <= 0.0 {
                    return 0.0;
                }
                let age = age_seconds as f64;
                let score = (-LN_2 * age / half_life).exp();
                if score.is_nan() {
                    0.0
                } else {
                    score.clamp(0.0, 1.0) as f32
                }
            }
        }
    }
}

/// Re-rank a list of candidates using the provided `TemporalConfig`.
///
/// - `candidates`: input candidate list (raw semantic scores must be in 0..1).
/// - `cfg`: temporal configuration (if `cfg.enabled == false` the function will
///   return results with `temporal_score = 0` and `final_score = raw_score`).
/// - `now_opt`: optional epoch seconds to use as the current time for deterministic tests.
///   If `None`, the function uses `SystemTime::now()` (UTC epoch seconds).
///
/// Returns candidates enriched with `temporal_score` and `final_score`, sorted by
/// `final_score` descending.
pub fn re_rank_with_temporal(
    candidates: Vec<Candidate>,
    cfg: &TemporalConfig,
    now_opt: Option<u64>,
) -> Vec<CandidateWithScores> {
    use rayon::prelude::*;

    // Resolve now in epoch seconds. Priority: now_opt (argument) > cfg.now_seconds > SystemTime.
    let now_seconds: u64 = now_opt.or(cfg.now_seconds).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    });

    // If temporal disabled, short-circuit to preserve raw_score semantics.
    if !cfg.enabled {
        let mut out: Vec<CandidateWithScores> = candidates
            .into_par_iter()
            .map(|c| {
                let raw = c.raw_score.clamp(0.0, 1.0);
                CandidateWithScores {
                    candidate: c,
                    temporal_score: 0.0,
                    final_score: raw,
                }
            })
            .collect();
        // sort by final_score descending
        out.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(Ordering::Equal)
        });
        return out;
    }

    // Compute scores using parallel iteration for better performance.
    let out: Vec<CandidateWithScores> = candidates
        .into_par_iter()
        .map(|c| {
            let temporal_score = compute_temporal_score(c.timestamp, now_seconds, cfg);
            let raw = c.raw_score.clamp(0.0, 1.0);
            // Weighted sum combination.
            let w = cfg.weight.clamp(0.0, 1.0);
            let final_score = (1.0 - w) * raw + w * temporal_score;
            CandidateWithScores {
                candidate: c,
                temporal_score,
                final_score: final_score.clamp(0.0, 1.0),
            }
        })
        .collect();

    // Sort by final_score descending (stable for deterministic outputs).
    let mut sorted_out = out;
    sorted_out.sort_by(|a, b| {
        b.final_score
            .partial_cmp(&a.final_score)
            .unwrap_or(Ordering::Equal)
    });

    sorted_out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to produce a deterministic 'now' value.
    fn fixed_now() -> u64 {
        // Use a constant epoch: 2026-01-01T00:00:00Z -> 1767225600
        1_767_225_600u64
    }

    #[test]
    fn test_exponential_half_life_behavior() {
        // Half-life 10 seconds: after 10s the score should be ~0.5
        let cfg = TemporalConfig {
            enabled: true,
            weight: 0.5, // irrelevant here
            mode: TemporalMode::Exponential,
            half_life_seconds: Some(10),
            window_seconds: None,
            boost_magnitude: None,
            buckets: None,
            now_seconds: None,
        };
        let now = 1_000_000u64;
        let candidate_time = now - 10;
        let sc = compute_temporal_score(Some(candidate_time), now, &cfg);
        // Allow small floating point tolerance.
        let diff = (sc as f64 - 0.5f64).abs();
        assert!(diff < 1e-6, "expected ~0.5, got {} (diff {})", sc, diff);
    }

    #[test]
    fn test_linear_window_behavior() {
        let cfg = TemporalConfig {
            enabled: true,
            weight: 0.5,
            mode: TemporalMode::LinearWindow,
            half_life_seconds: None,
            window_seconds: Some(100),
            boost_magnitude: None,
            buckets: None,
            now_seconds: None,
        };
        let now = 200u64;
        let candidate_time = 150u64; // age 50 -> normalized 1 - 50/100 = 0.5
        let sc = compute_temporal_score(Some(candidate_time), now, &cfg);
        assert!((sc as f64 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_step_mode_behavior() {
        let cfg = TemporalConfig {
            enabled: true,
            weight: 0.5,
            mode: TemporalMode::Step,
            half_life_seconds: None,
            window_seconds: Some(3600), // 1 hour
            boost_magnitude: Some(0.8),
            buckets: None,
            now_seconds: None,
        };
        let now = 10_000u64;
        let candidate_recent = now - 1800; // within window
        let candidate_old = now - 7200; // outside window
        let sc_recent = compute_temporal_score(Some(candidate_recent), now, &cfg);
        let sc_old = compute_temporal_score(Some(candidate_old), now, &cfg);
        assert!((sc_recent - 0.8).abs() < 1e-6);
        assert!((sc_old - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_buckets_mode_behavior() {
        let cfg = TemporalConfig {
            enabled: true,
            weight: 0.5,
            mode: TemporalMode::Buckets,
            half_life_seconds: None,
            window_seconds: None,
            boost_magnitude: None,
            buckets: Some(vec![(86400, 1.0), (7 * 86400, 0.6), (30 * 86400, 0.3)]),
            now_seconds: None,
        };
        let now = 10_000_000u64; // Use a larger now value to avoid overflow
        let c1 = now - 3600; // 1 hour -> bucket 86400 -> 1.0
        let c2 = now - (3 * 86400); // 3 days -> bucket 7d -> 0.6
        let c3 = now - (20 * 86400); // 20 days -> bucket 30d -> 0.3
        let c4 = now - (100 * 86400); // > 30d -> 0.0
        assert!((compute_temporal_score(Some(c1), now, &cfg) - 1.0).abs() < 1e-6);
        assert!((compute_temporal_score(Some(c2), now, &cfg) - 0.6).abs() < 1e-6);
        assert!((compute_temporal_score(Some(c3), now, &cfg) - 0.3).abs() < 1e-6);
        assert!((compute_temporal_score(Some(c4), now, &cfg) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_missing_timestamp_yields_zero_temporal_score() {
        let cfg = TemporalConfig::default();
        let now = fixed_now();
        let sc = compute_temporal_score(None, now, &cfg);
        assert!((sc - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_re_rank_with_temporal_changes_ordering() {
        // Prepare three candidates (A,B,C) with raw scores favoring A but age favoring B/C.
        let now = fixed_now();
        // Ages: A is old (60 days), B recent (2 days), C recent (1 day)
        let age_a = 60u64 * 24 * 3600;
        let age_b = 2u64 * 24 * 3600;
        let age_c = 24 * 3600;
        let a_ts = now.saturating_sub(age_a);
        let b_ts = now.saturating_sub(age_b);
        let c_ts = now.saturating_sub(age_c);

        let a = Candidate {
            turn_id: 1,
            note_id: 1,
            raw_score: 0.90,
            timestamp: Some(a_ts),
        };
        let b = Candidate {
            turn_id: 2,
            note_id: 2,
            raw_score: 0.85,
            timestamp: Some(b_ts),
        };
        let c = Candidate {
            turn_id: 3,
            note_id: 3,
            raw_score: 0.80,
            timestamp: Some(c_ts),
        };

        // Use a moderate weight so temporal effect can flip ordering for A vs B/C.
        let cfg = TemporalConfig {
            enabled: true,
            weight: 0.30,
            mode: TemporalMode::Exponential,
            half_life_seconds: Some(days_to_seconds(14.0)),
            window_seconds: None,
            boost_magnitude: None,
            buckets: None,
            now_seconds: None,
        };

        let results = re_rank_with_temporal(vec![a.clone(), b.clone(), c.clone()], &cfg, Some(now));

        // Ensure we have three results sorted by final_score descending.
        assert_eq!(results.len(), 3);

        // The top candidate should NOT be A (the old, highest raw score) for these params.
        let top_turn = results[0].turn_id();
        assert_ne!(
            top_turn, a.turn_id,
            "expected temporal re-ranking to demote the oldest candidate"
        );

        // Verify final_score monotonicity
        for w in results.windows(2) {
            assert!(
                w[0].final_score >= w[1].final_score,
                "results are not sorted descending by final_score"
            );
        }
    }

    #[test]
    fn test_disabled_temporal_preserves_raw_order() {
        let now = fixed_now();
        let a = Candidate {
            turn_id: 1,
            note_id: 1,
            raw_score: 0.70,
            timestamp: Some(now - 10),
        };
        let b = Candidate {
            turn_id: 2,
            note_id: 2,
            raw_score: 0.90,
            timestamp: Some(now - 10000),
        };
        // temporal disabled
        let cfg = TemporalConfig {
            enabled: false,
            weight: 1.0,
            mode: TemporalMode::Exponential,
            half_life_seconds: Some(10),
            window_seconds: None,
            boost_magnitude: None,
            buckets: None,
            now_seconds: None,
        };
        let results = re_rank_with_temporal(vec![a.clone(), b.clone()], &cfg, Some(now));
        assert_eq!(results.len(), 2);
        // b (raw 0.9) should be first.
        assert_eq!(results[0].turn_id(), b.turn_id);
        assert!((results[0].final_score - b.raw_score).abs() < 1e-6);
    }
}
