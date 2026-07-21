use std::collections::HashMap;

pub fn map_legacy_nsfw_ec(final_top_category: &str) -> &'static str {
    match final_top_category {
        "porn" => "explicit",
        "nudity" => "nudity",
        "suggestive" => "provocative",
        "sexual_minor_content" => "explicit",
        _ => "neutral",
    }
}

pub fn map_legacy_nsfw_gore(max_category_severities: &HashMap<String, u8>) -> &'static str {
    let gore = max_category_severities.get("gore").copied().unwrap_or(0);
    let violence = max_category_severities
        .get("violence")
        .copied()
        .unwrap_or(0);
    match gore.max(violence) {
        s if s >= 5 => "VERY_LIKELY",
        s if s >= 4 => "LIKELY",
        s if s >= 3 => "POSSIBLE",
        s if s >= 1 => "UNLIKELY",
        _ => "VERY_UNLIKELY",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[rstest::rstest]
    #[case("porn", "explicit")]
    #[case("nudity", "nudity")]
    #[case("suggestive", "provocative")]
    #[case("sexual_minor_content", "explicit")]
    #[case("gore", "neutral")]
    #[case("safe", "neutral")]
    fn maps_final_top_category_to_legacy_nsfw_ec(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(map_legacy_nsfw_ec(input), expected);
    }

    fn severities(pairs: &[(&str, u8)]) -> HashMap<String, u8> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[rstest::rstest]
    #[case(&[("gore", 5)], "VERY_LIKELY")]
    #[case(&[("violence", 5)], "VERY_LIKELY")]
    #[case(&[("gore", 4)], "LIKELY")]
    #[case(&[("gore", 3)], "POSSIBLE")]
    #[case(&[("gore", 1)], "UNLIKELY")]
    #[case(&[], "VERY_UNLIKELY")]
    #[case(&[("gore", 2), ("violence", 4)], "LIKELY")] // max(gore, violence) wins
    fn maps_max_gore_violence_severity_to_legacy_nsfw_gore(
        #[case] input: &[(&str, u8)],
        #[case] expected: &str,
    ) {
        assert_eq!(map_legacy_nsfw_gore(&severities(input)), expected);
    }
}
