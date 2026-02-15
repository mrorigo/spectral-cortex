// Rust guideline compliant 2026-02-14
use anyhow::Result;

use super::IngestArgs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitSplitMode {
    Off,
    Auto,
    Strict,
}

impl CommitSplitMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Auto => "auto",
            Self::Strict => "strict",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseMode {
    ConventionalHeader,
    BulletGrouped,
    ParagraphFallback,
}

#[derive(Debug, Clone)]
pub(crate) struct CommitSegment {
    header: String,
    details: Vec<String>,
    confidence: f32,
    parse_mode: ParseMode,
}

impl CommitSegment {
    pub(crate) fn to_content(&self) -> String {
        let mut out = String::new();
        out.push_str(self.header.trim());
        for detail in &self.details {
            let trimmed = detail.trim();
            if !trimmed.is_empty() {
                out.push('\n');
                out.push_str(trimmed);
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CommitSplitConfig {
    pub(crate) mode: CommitSplitMode,
    max_segments: usize,
    min_confidence: f32,
}

impl CommitSplitConfig {
    pub(crate) fn from_ingest_args(args: &IngestArgs) -> Result<Self> {
        let mode = match args.git_commit_split_mode.to_lowercase().as_str() {
            "off" => CommitSplitMode::Off,
            "auto" => CommitSplitMode::Auto,
            "strict" => CommitSplitMode::Strict,
            other => {
                return Err(anyhow::anyhow!(
                    "unsupported --git-commit-split-mode '{}'; supported: off|auto|strict",
                    other
                ));
            }
        };
        let max_segments = args.git_commit_split_max_segments.max(1);
        let min_confidence = args.git_commit_split_min_confidence.clamp(0.0, 1.0);
        Ok(Self {
            mode,
            max_segments,
            min_confidence,
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct CommitSplitStats {
    pub(crate) commits_seen: usize,
    pub(crate) commits_split: usize,
    pub(crate) total_segments_emitted: usize,
    pub(crate) fallback_to_single: usize,
    pub(crate) segments_from_headers: usize,
    pub(crate) segments_from_bullets: usize,
    pub(crate) segments_from_paragraphs: usize,
}

fn is_conventional_header(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    let Some(colon_pos) = trimmed.find(':') else {
        return false;
    };
    if colon_pos == 0 || colon_pos + 1 >= trimmed.len() {
        return false;
    }

    let prefix = &trimmed[..colon_pos];
    let message = trimmed[colon_pos + 1..].trim();
    if message.is_empty() {
        return false;
    }

    let Some(first) = prefix.chars().next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }

    prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '(' | ')' | '-' | '_' | '/'))
}

fn split_by_conventional_headers(
    lines: &[&str],
    max_segments: usize,
) -> Option<Vec<CommitSegment>> {
    let mut headers: Vec<usize> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if is_conventional_header(line) {
            headers.push(idx);
        }
    }
    if headers.len() < 2 {
        return None;
    }

    let mut segments: Vec<CommitSegment> = Vec::new();
    for (hidx, start_idx) in headers.iter().enumerate() {
        if segments.len() >= max_segments {
            break;
        }
        let end_idx = headers.get(hidx + 1).copied().unwrap_or(lines.len());
        let header = lines[*start_idx].trim().to_string();
        let mut details = Vec::new();
        for line in lines.iter().take(end_idx).skip(*start_idx + 1) {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                details.push(trimmed.to_string());
            }
        }
        segments.push(CommitSegment {
            header,
            details,
            confidence: 0.95,
            parse_mode: ParseMode::ConventionalHeader,
        });
    }

    if segments.len() >= 2 {
        Some(segments)
    } else {
        None
    }
}

fn is_bullet_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && (trimmed.starts_with("- ") || trimmed.starts_with("* "))
}

fn split_by_bullets(lines: &[&str], max_segments: usize) -> Option<Vec<CommitSegment>> {
    let bullets: Vec<String> = lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| is_bullet_line(line))
        .map(|line| line[2..].trim().to_string())
        .filter(|line| line.len() >= 8)
        .collect();

    if bullets.len() < 2 {
        return None;
    }

    let mut segments = Vec::new();
    for bullet in bullets.into_iter().take(max_segments) {
        segments.push(CommitSegment {
            header: bullet,
            details: Vec::new(),
            confidence: 0.8,
            parse_mode: ParseMode::BulletGrouped,
        });
    }
    if segments.len() >= 2 {
        Some(segments)
    } else {
        None
    }
}

fn split_by_paragraphs(message: &str, max_segments: usize) -> Option<Vec<CommitSegment>> {
    let paras: Vec<String> = message
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(ToString::to_string)
        .collect();

    if paras.len() < 2 {
        return None;
    }

    let substantial = paras
        .iter()
        .filter(|p| p.split_whitespace().count() >= 4)
        .count();
    if substantial < 2 {
        return None;
    }

    let mut segments = Vec::new();
    for para in paras.into_iter().take(max_segments) {
        let mut lines = para.lines().map(str::trim).filter(|l| !l.is_empty());
        let Some(header) = lines.next() else {
            continue;
        };
        let details: Vec<String> = lines.map(ToString::to_string).collect();
        segments.push(CommitSegment {
            header: header.to_string(),
            details,
            confidence: 0.65,
            parse_mode: ParseMode::ParagraphFallback,
        });
    }
    if segments.len() >= 2 {
        Some(segments)
    } else {
        None
    }
}

fn fallback_single_segment(message: &str) -> Vec<CommitSegment> {
    let mut lines = message.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = lines.next().unwrap_or(message).trim().to_string();
    let details: Vec<String> = lines.map(ToString::to_string).collect();
    vec![CommitSegment {
        header,
        details,
        confidence: 1.0,
        parse_mode: ParseMode::ParagraphFallback,
    }]
}

pub(crate) fn split_commit_message(
    message: &str,
    config: &CommitSplitConfig,
    stats: &mut CommitSplitStats,
) -> Vec<CommitSegment> {
    stats.commits_seen = stats.commits_seen.saturating_add(1);

    if config.mode == CommitSplitMode::Off {
        stats.total_segments_emitted = stats.total_segments_emitted.saturating_add(1);
        stats.fallback_to_single = stats.fallback_to_single.saturating_add(1);
        return fallback_single_segment(message);
    }

    let lines: Vec<&str> = message.lines().collect();
    let candidate = split_by_conventional_headers(&lines, config.max_segments)
        .or_else(|| split_by_bullets(&lines, config.max_segments))
        .or_else(|| split_by_paragraphs(message, config.max_segments));

    let Some(segments) = candidate else {
        stats.total_segments_emitted = stats.total_segments_emitted.saturating_add(1);
        stats.fallback_to_single = stats.fallback_to_single.saturating_add(1);
        return fallback_single_segment(message);
    };

    let avg_conf = segments.iter().map(|s| s.confidence).sum::<f32>() / segments.len() as f32;
    let should_keep_split = match config.mode {
        CommitSplitMode::Strict => true,
        CommitSplitMode::Auto => avg_conf >= config.min_confidence,
        CommitSplitMode::Off => false,
    };

    if !should_keep_split {
        stats.total_segments_emitted = stats.total_segments_emitted.saturating_add(1);
        stats.fallback_to_single = stats.fallback_to_single.saturating_add(1);
        return fallback_single_segment(message);
    }

    stats.commits_split = stats.commits_split.saturating_add(1);
    stats.total_segments_emitted = stats.total_segments_emitted.saturating_add(segments.len());
    for seg in &segments {
        match seg.parse_mode {
            ParseMode::ConventionalHeader => {
                stats.segments_from_headers = stats.segments_from_headers.saturating_add(1)
            }
            ParseMode::BulletGrouped => {
                stats.segments_from_bullets = stats.segments_from_bullets.saturating_add(1)
            }
            ParseMode::ParagraphFallback => {
                stats.segments_from_paragraphs = stats.segments_from_paragraphs.saturating_add(1)
            }
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::{split_commit_message, CommitSplitConfig, CommitSplitMode, CommitSplitStats};

    fn cfg(mode: CommitSplitMode) -> CommitSplitConfig {
        CommitSplitConfig {
            mode,
            max_segments: 6,
            min_confidence: 0.75,
        }
    }

    #[test]
    fn split_off_keeps_single_segment() {
        let mut stats = CommitSplitStats::default();
        let message = "refactor: use dependency injection\nfix: add failure logs";
        let parts = split_commit_message(message, &cfg(CommitSplitMode::Off), &mut stats);
        assert_eq!(parts.len(), 1);
        assert_eq!(stats.commits_split, 0);
        assert_eq!(stats.fallback_to_single, 1);
    }

    #[test]
    fn split_auto_detects_multiple_headers() {
        let mut stats = CommitSplitStats::default();
        let message =
            "refactor: use dependency injection\n- inject payment service\nfix: add failure logs";
        let parts = split_commit_message(message, &cfg(CommitSplitMode::Auto), &mut stats);
        assert_eq!(parts.len(), 2);
        assert_eq!(stats.commits_split, 1);
        assert_eq!(stats.total_segments_emitted, 2);
    }

    #[test]
    fn split_auto_keeps_single_when_confidence_too_low() {
        let mut stats = CommitSplitStats::default();
        let message =
            "first paragraph has several words for context\n\nsecond paragraph is also substantial text";
        let strict_conf = CommitSplitConfig {
            mode: CommitSplitMode::Auto,
            max_segments: 6,
            min_confidence: 0.9,
        };
        let parts = split_commit_message(message, &strict_conf, &mut stats);
        assert_eq!(parts.len(), 1);
        assert_eq!(stats.commits_split, 0);
        assert_eq!(stats.fallback_to_single, 1);
    }

    #[test]
    fn split_strict_accepts_paragraph_fallback() {
        let mut stats = CommitSplitStats::default();
        let message =
            "first paragraph has several words for context\n\nsecond paragraph is also substantial text";
        let parts = split_commit_message(message, &cfg(CommitSplitMode::Strict), &mut stats);
        assert_eq!(parts.len(), 2);
        assert_eq!(stats.commits_split, 1);
    }

    #[test]
    fn split_respects_segment_cap() {
        let mut stats = CommitSplitStats::default();
        let message = "fix: one\nfix: two\nfix: three\nfix: four";
        let limited = CommitSplitConfig {
            mode: CommitSplitMode::Strict,
            max_segments: 2,
            min_confidence: 0.75,
        };
        let parts = split_commit_message(message, &limited, &mut stats);
        assert_eq!(parts.len(), 2);
        assert_eq!(stats.total_segments_emitted, 2);
    }
}
