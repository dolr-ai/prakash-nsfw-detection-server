use std::collections::HashMap;

pub const MODERATION_CATEGORIES: [&str; 11] = [
    "safe",
    "suggestive",
    "nudity",
    "porn",
    "gore",
    "violence",
    "self_harm",
    "hate_or_extremism",
    "drugs",
    "unknown",
    "sexual_minor_content",
];

/// Ground truth is the actual Python code, not the Python repo's own `plan.md`
/// (which describes a different, unimplemented rule). See design spec §5/§7.1.
pub const CATEGORY_BLOCK_THRESHOLDS: &[(&str, u8)] = &[
    ("sexual_minor_content", 3),
    ("porn", 4),
    ("gore", 4),
    ("violence", 4),
    ("self_harm", 4),
    ("hate_or_extremism", 4),
    ("drugs", 4),
    ("unknown", 4),
    ("suggestive", 5),
    ("nudity", 5),
    // "safe" is deliberately absent -- never triggers is_nsfw
];

pub const RISK_ORDER: [&str; 11] = [
    "sexual_minor_content",
    "porn",
    "nudity",
    "gore",
    "violence",
    "self_harm",
    "hate_or_extremism",
    "drugs",
    "suggestive",
    "unknown",
    "safe",
];

pub fn compute_is_nsfw(categories: &HashMap<String, u8>) -> bool {
    CATEGORY_BLOCK_THRESHOLDS
        .iter()
        .any(|(cat, threshold)| categories.get(*cat).copied().unwrap_or(0) >= *threshold)
}

pub fn compute_overall_severity(
    top_category: &str,
    categories: &HashMap<String, u8>,
) -> Option<u8> {
    if !MODERATION_CATEGORIES.contains(&top_category) {
        return None;
    }
    Some(categories.get(top_category).copied().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn categories(pairs: &[(&str, u8)]) -> HashMap<String, u8> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[rstest::rstest]
    #[case(&[("porn", 4)], true)]
    #[case(&[("sexual_minor_content", 3)], true)]
    #[case(&[("gore", 4)], true)]
    #[case(&[("nudity", 4), ("suggestive", 4)], false)]
    #[case(&[("nudity", 5)], true)]
    #[case(&[("safe", 5)], false)]
    fn compute_is_nsfw_matches_category_thresholds(
        #[case] input: &[(&str, u8)],
        #[case] expected: bool,
    ) {
        assert_eq!(compute_is_nsfw(&categories(input)), expected);
    }

    #[test]
    fn compute_overall_severity_returns_top_category_score() {
        let cats = categories(&[("porn", 4), ("gore", 2)]);
        assert_eq!(compute_overall_severity("porn", &cats), Some(4));
    }

    #[test]
    fn compute_overall_severity_returns_none_for_unknown_category() {
        let cats = categories(&[("porn", 4)]);
        assert_eq!(compute_overall_severity("not_a_real_category", &cats), None);
    }

    #[test]
    fn category_block_thresholds_cover_all_ten_unsafe_categories() {
        // MODERATION_CATEGORIES has 11 entries; every one except "safe" must have a threshold.
        let thresholded: Vec<&str> = CATEGORY_BLOCK_THRESHOLDS.iter().map(|(c, _)| *c).collect();
        for cat in MODERATION_CATEGORIES.iter().filter(|c| **c != "safe") {
            assert!(thresholded.contains(cat), "missing threshold for {cat}");
        }
        assert!(
            !thresholded.contains(&"safe"),
            "safe must never have a block threshold"
        );
    }
}
