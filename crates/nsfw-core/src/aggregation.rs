use crate::legacy_mapping::{map_legacy_nsfw_ec, map_legacy_nsfw_gore};
use crate::moderation::{MODERATION_CATEGORIES, RISK_ORDER};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AggregationInputFrame {
    pub top_category: String,
    pub overall_severity: u8,
    pub categories: HashMap<String, u8>,
    pub is_nsfw: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AggregationError {
    #[error("cannot aggregate an empty frame list")]
    EmptyFrameList,
}

#[derive(Debug, Clone)]
pub struct AggregationOutput {
    pub max_category_severities: HashMap<String, u8>,
    pub nsfw_frame_count: i32,
    pub max_overall_severity: u8,
    pub final_is_nsfw: bool,
    pub final_score: f64,
    pub final_top_category: String,
    pub move_required: bool,
    pub legacy_nsfw_ec: String,
    pub legacy_nsfw_gore: String,
}

/// Mirrors `AggregationService.aggregate` (Python). A non-empty-frames precondition
/// failure is a `Result::Err` here, not a panic -- and per spec §10, callers must NOT
/// treat this as unconditionally terminal: in Python it's an unclassified exception
/// that falls into the default-retryable bucket, subject to the normal attempts budget.
pub fn aggregate(
    frames: &[AggregationInputFrame],
    move_threshold: f64,
) -> Result<AggregationOutput, AggregationError> {
    if frames.is_empty() {
        return Err(AggregationError::EmptyFrameList);
    }

    let mut max_category_severities = HashMap::new();
    for cat in MODERATION_CATEGORIES {
        let max = frames
            .iter()
            .map(|f| f.categories.get(cat).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        max_category_severities.insert(cat.to_string(), max);
    }

    let nsfw_frame_count = frames.iter().filter(|f| f.is_nsfw).count() as i32;
    let max_overall_severity = frames.iter().map(|f| f.overall_severity).max().unwrap_or(0);
    let final_is_nsfw = nsfw_frame_count > 0;
    let final_score = max_overall_severity as f64 / 5.0;
    let final_top_category = select_top_category(frames, max_overall_severity);
    let move_required = final_score >= move_threshold;
    let legacy_nsfw_ec = map_legacy_nsfw_ec(&final_top_category).to_string();
    let legacy_nsfw_gore = map_legacy_nsfw_gore(&max_category_severities).to_string();

    Ok(AggregationOutput {
        max_category_severities,
        nsfw_frame_count,
        max_overall_severity,
        final_is_nsfw,
        final_score,
        final_top_category,
        move_required,
        legacy_nsfw_ec,
        legacy_nsfw_gore,
    })
}

fn risk_rank(category: &str) -> usize {
    RISK_ORDER
        .iter()
        .position(|c| *c == category)
        .unwrap_or(RISK_ORDER.len())
}

/// Highest severity wins; among frames tied at the highest severity, the
/// riskiest category (by RISK_ORDER) wins. Matches Python's `_select_top_category`.
fn select_top_category(frames: &[AggregationInputFrame], highest_severity: u8) -> String {
    frames
        .iter()
        .filter(|f| f.overall_severity == highest_severity)
        .map(|f| f.top_category.as_str())
        .min_by_key(|cat| risk_rank(cat))
        .unwrap_or("safe")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    // Note: no `use std::collections::HashMap` here -- `.collect()` below infers the
    // type from `AggregationInputFrame::categories`'s field type, so naming it directly
    // would trip clippy::unused_imports.

    fn frame(
        top_category: &str,
        overall_severity: u8,
        cats: &[(&str, u8)],
        is_nsfw: bool,
    ) -> AggregationInputFrame {
        AggregationInputFrame {
            top_category: top_category.to_string(),
            overall_severity,
            categories: cats.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            is_nsfw,
        }
    }

    #[test]
    fn empty_frame_list_is_an_error_not_a_panic() {
        let result = aggregate(&[], 0.8);
        assert!(matches!(result, Err(AggregationError::EmptyFrameList)));
    }

    #[test]
    fn one_safe_frame_is_not_nsfw_with_zero_score() {
        let frames = vec![frame("safe", 0, &[], false)];
        let out = aggregate(&frames, 0.8).unwrap();
        assert!(!out.final_is_nsfw);
        assert_eq!(out.final_score, 0.0);
        assert!(!out.move_required);
    }

    #[test]
    fn one_nsfw_frame_among_safe_frames_flags_the_whole_video() {
        let frames = vec![
            frame("safe", 0, &[], false),
            frame("porn", 4, &[("porn", 4)], true),
            frame("safe", 0, &[], false),
        ];
        let out = aggregate(&frames, 0.8).unwrap();
        assert!(
            out.final_is_nsfw,
            "one bad frame must flag the whole video -- no averaging"
        );
    }

    #[test]
    fn severity_four_produces_score_point_eight_and_requires_move() {
        let frames = vec![frame("porn", 4, &[("porn", 4)], true)];
        let out = aggregate(&frames, 0.8).unwrap();
        assert_eq!(out.final_score, 0.8);
        assert!(out.move_required);
    }

    #[test]
    fn severity_three_flags_nsfw_but_does_not_require_move() {
        // sexual_minor_content's block threshold is 3 (the lowest of any category).
        let frames = vec![frame(
            "sexual_minor_content",
            3,
            &[("sexual_minor_content", 3)],
            true,
        )];
        let out = aggregate(&frames, 0.8).unwrap();
        assert!(out.final_is_nsfw);
        assert_eq!(out.final_score, 0.6);
        assert!(!out.move_required);
    }

    #[test]
    fn tie_break_at_equal_severity_picks_the_higher_risk_category() {
        let frames = vec![
            frame("drugs", 4, &[("drugs", 4)], true),
            frame("gore", 4, &[("gore", 4)], true),
        ];
        let out = aggregate(&frames, 0.8).unwrap();
        // RISK_ORDER ranks gore above drugs.
        assert_eq!(out.final_top_category, "gore");
    }

    #[test]
    fn sexual_minor_content_always_wins_the_tie_break() {
        let frames = vec![
            frame("porn", 5, &[("porn", 5)], true),
            frame(
                "sexual_minor_content",
                5,
                &[("sexual_minor_content", 5)],
                true,
            ),
        ];
        let out = aggregate(&frames, 0.8).unwrap();
        assert_eq!(out.final_top_category, "sexual_minor_content");
    }

    #[test]
    fn legacy_fields_are_derived_from_the_final_result() {
        let frames = vec![frame("porn", 5, &[("porn", 5)], true)];
        let out = aggregate(&frames, 0.8).unwrap();
        assert_eq!(out.legacy_nsfw_ec, "explicit");
        assert_eq!(out.legacy_nsfw_gore, "VERY_UNLIKELY"); // no gore/violence severity present
    }

    #[test]
    fn max_category_severities_covers_all_eleven_categories_across_frames() {
        let frames = vec![
            frame("porn", 4, &[("porn", 4), ("gore", 2)], true),
            frame("gore", 3, &[("gore", 3)], false),
        ];
        let out = aggregate(&frames, 0.8).unwrap();
        assert_eq!(out.max_category_severities.get("porn"), Some(&4));
        assert_eq!(out.max_category_severities.get("gore"), Some(&3)); // max across frames
        assert_eq!(out.max_category_severities.len(), 11);
    }
}
