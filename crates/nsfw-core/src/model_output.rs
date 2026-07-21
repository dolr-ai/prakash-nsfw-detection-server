use crate::moderation::{MODERATION_CATEGORIES, compute_is_nsfw};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelOutputError {
    #[error("model response is not valid JSON")]
    InvalidJson,
    #[error("model response does not match the expected schema")]
    InvalidSchema,
}

#[derive(Debug, Clone)]
pub struct ModerationModelOutput {
    pub top_category: String,
    pub categories: HashMap<String, u8>,
    pub reason: String,
    pub overall_severity: u8,
    pub is_nsfw: bool,
}

impl ModerationModelOutput {
    pub fn parse(
        top_category: String,
        categories: HashMap<String, u8>,
        reason: String,
    ) -> Result<Self, ModelOutputError> {
        validate_categories(&categories)?;
        if !MODERATION_CATEGORIES.contains(&top_category.as_str()) {
            return Err(ModelOutputError::InvalidSchema);
        }
        validate_top_category_matches_scores(&top_category, &categories)?;

        let overall_severity = categories.get(top_category.as_str()).copied().unwrap_or(0);
        let is_nsfw = compute_is_nsfw(&categories);
        Ok(Self {
            top_category,
            categories,
            reason,
            overall_severity,
            is_nsfw,
        })
    }
}

fn validate_categories(categories: &HashMap<String, u8>) -> Result<(), ModelOutputError> {
    if categories.len() != MODERATION_CATEGORIES.len() {
        return Err(ModelOutputError::InvalidSchema);
    }
    for cat in MODERATION_CATEGORIES {
        match categories.get(cat) {
            Some(v) if *v <= 5 => {}
            _ => return Err(ModelOutputError::InvalidSchema),
        }
    }
    Ok(())
}

fn validate_top_category_matches_scores(
    top_category: &str,
    categories: &HashMap<String, u8>,
) -> Result<(), ModelOutputError> {
    let max_unsafe = categories
        .iter()
        .filter(|(cat, _)| cat.as_str() != "safe")
        .map(|(_, v)| *v)
        .max()
        .unwrap_or(0);

    if top_category == "safe" {
        return if max_unsafe == 0 {
            Ok(())
        } else {
            Err(ModelOutputError::InvalidSchema)
        };
    }

    let top_score = categories.get(top_category).copied().unwrap_or(0);
    if top_score == 0 || top_score != max_unsafe {
        return Err(ModelOutputError::InvalidSchema);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn full_categories(overrides: &[(&str, u8)]) -> HashMap<String, u8> {
        let mut cats: HashMap<String, u8> = crate::moderation::MODERATION_CATEGORIES
            .iter()
            .map(|c| (c.to_string(), 0))
            .collect();
        for (k, v) in overrides {
            cats.insert(k.to_string(), *v);
        }
        cats
    }

    #[test]
    fn accepts_a_safe_frame_with_all_zero_categories() {
        let cats = full_categories(&[]);
        let output =
            ModerationModelOutput::parse("safe".to_string(), cats, "nothing visible".to_string());
        assert!(output.is_ok());
        let output = output.unwrap();
        assert!(!output.is_nsfw);
        assert_eq!(output.overall_severity, 0);
    }

    #[test]
    fn accepts_top_category_matching_the_max_unsafe_severity() {
        let cats = full_categories(&[("porn", 4), ("gore", 2)]);
        let output =
            ModerationModelOutput::parse("porn".to_string(), cats, "explicit content".to_string())
                .unwrap();
        assert_eq!(output.overall_severity, 4);
        assert!(output.is_nsfw);
    }

    #[test]
    fn rejects_top_category_whose_score_is_lower_than_another_categorys() {
        // porn=2 but gore=4 is higher -- top_category must be the max, not just nonzero.
        let cats = full_categories(&[("porn", 2), ("gore", 4)]);
        let result = ModerationModelOutput::parse("porn".to_string(), cats, "x".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_safe_top_category_when_an_unsafe_category_is_nonzero() {
        let cats = full_categories(&[("porn", 1)]);
        let result = ModerationModelOutput::parse("safe".to_string(), cats, "x".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_category_keys() {
        let mut cats = full_categories(&[]);
        cats.remove("gore");
        let result = ModerationModelOutput::parse("safe".to_string(), cats, "x".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_out_of_range_severity() {
        let cats = full_categories(&[("porn", 6)]);
        let result = ModerationModelOutput::parse("porn".to_string(), cats, "x".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_top_category() {
        let cats = full_categories(&[]);
        let result =
            ModerationModelOutput::parse("not_a_category".to_string(), cats, "x".to_string());
        assert!(result.is_err());
    }
}
