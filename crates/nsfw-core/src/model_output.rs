use crate::moderation::{MODERATION_CATEGORIES, compute_is_nsfw};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelOutputError {
    #[error("model response is not valid JSON")]
    InvalidJson,
    #[error("model response does not match the expected schema")]
    InvalidSchema,
}

/// `Serialize` is derived here (not just used internally) because this struct doubles
/// as the exact wire response for the stateless `/v1/images/*` and `/v1/text/detect`
/// endpoints (spec §9.2's `ModerationDetectResponse`) -- its fields already match that
/// wire contract 1:1, and its `parse()` constructor already guarantees the
/// self-consistency Python's `ModerationDetectResponse.validate_policy_fields` checks
/// separately, so no redundant response DTO/validator is needed in the API layer.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

#[derive(Debug, Clone)]
pub struct FrameModerationOutput {
    pub base: ModerationModelOutput,
    pub frame_index: i32,
}

pub type TextModerationOutput = ModerationModelOutput;

/// Scans `raw` for the first complete JSON document. If a second, independent JSON
/// value follows it in the same string, that's treated as ambiguous input and rejected
/// -- matching Python's `_extract_single_json_document` behavior exactly.
fn extract_single_json_document(raw: &str) -> Result<serde_json::Value, ModelOutputError> {
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(value);
    }
    let mut stream = serde_json::Deserializer::from_str(trimmed).into_iter::<serde_json::Value>();
    let first = stream
        .next()
        .ok_or(ModelOutputError::InvalidJson)?
        .map_err(|_| ModelOutputError::InvalidJson)?;
    if stream.next().is_some() {
        return Err(ModelOutputError::InvalidJson);
    }
    Ok(first)
}

fn unwrap_envelope(value: serde_json::Value, keys: &[&str]) -> serde_json::Value {
    let unwrapped = value
        .as_object()
        .filter(|map| map.len() == 1)
        .and_then(|map| {
            map.iter()
                .next()
                .filter(|(key, _)| keys.contains(&key.as_str()))
                .map(|(_, inner)| inner.clone())
        });
    unwrapped.unwrap_or(value)
}

fn parse_categories_object(
    value: Option<&serde_json::Value>,
) -> Result<HashMap<String, u8>, ModelOutputError> {
    let obj = value
        .and_then(|v| v.as_object())
        .ok_or(ModelOutputError::InvalidSchema)?;
    let mut categories = HashMap::new();
    for (key, val) in obj {
        if val.is_boolean() {
            return Err(ModelOutputError::InvalidSchema);
        }
        let n = val.as_u64().ok_or(ModelOutputError::InvalidSchema)?;
        if n > 5 {
            return Err(ModelOutputError::InvalidSchema);
        }
        categories.insert(key.clone(), n as u8);
    }
    Ok(categories)
}

pub fn parse_visual_batch_response(
    raw_response: &str,
    expected_count: usize,
) -> Result<Vec<FrameModerationOutput>, ModelOutputError> {
    let value = extract_single_json_document(raw_response)?;
    let value = unwrap_envelope(value, &["results", "frames", "result"]);

    let items: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(arr) => arr,
        obj @ serde_json::Value::Object(_) if expected_count == 1 => vec![obj],
        _ => return Err(ModelOutputError::InvalidSchema),
    };
    if items.len() != expected_count {
        return Err(ModelOutputError::InvalidSchema);
    }

    items
        .into_iter()
        .enumerate()
        .map(|(position, item)| parse_frame_item(item, position))
        .collect()
}

fn parse_frame_item(
    item: serde_json::Value,
    position: usize,
) -> Result<FrameModerationOutput, ModelOutputError> {
    let obj = item.as_object().ok_or(ModelOutputError::InvalidSchema)?;
    let frame_index = obj
        .get("frame_index")
        .and_then(|v| v.as_i64())
        .ok_or(ModelOutputError::InvalidSchema)? as i32;
    if frame_index as usize != position {
        return Err(ModelOutputError::InvalidSchema);
    }
    let top_category = obj
        .get("top_category")
        .and_then(|v| v.as_str())
        .ok_or(ModelOutputError::InvalidSchema)?
        .to_string();
    let reason = obj
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let categories = parse_categories_object(obj.get("categories"))?;

    let base = ModerationModelOutput::parse(top_category, categories, reason)?;
    Ok(FrameModerationOutput { base, frame_index })
}

pub fn parse_text_moderation_response(
    raw_response: &str,
) -> Result<TextModerationOutput, ModelOutputError> {
    let value = extract_single_json_document(raw_response)?;
    let value = unwrap_envelope(value, &["result", "moderation"]);
    let obj = value.as_object().ok_or(ModelOutputError::InvalidSchema)?;

    let top_category = obj
        .get("top_category")
        .and_then(|v| v.as_str())
        .ok_or(ModelOutputError::InvalidSchema)?
        .to_string();
    let reason = obj
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let categories = parse_categories_object(obj.get("categories"))?;

    ModerationModelOutput::parse(top_category, categories, reason)
}

#[cfg(test)]
mod batch_and_text_parsing_tests {
    use super::*;

    fn valid_frame_json(frame_index: u32, top_category: &str, score: u8) -> String {
        format!(
            r#"{{"frame_index": {frame_index}, "top_category": "{top_category}", "reason": "x",
                "categories": {{"safe":0,"suggestive":0,"nudity":0,"porn":{score},"gore":0,"violence":0,
                "self_harm":0,"hate_or_extremism":0,"drugs":0,"unknown":0,"sexual_minor_content":0}}}}"#
        )
    }

    #[test]
    fn parses_a_single_frame_batch() {
        let raw = format!("[{}]", valid_frame_json(0, "porn", 4));
        let frames = parse_visual_batch_response(&raw, 1).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].frame_index, 0);
        assert_eq!(frames[0].base.overall_severity, 4);
    }

    #[test]
    fn parses_a_five_frame_batch_preserving_order() {
        let items: Vec<String> = (0..5).map(|i| valid_frame_json(i, "safe", 0)).collect();
        let raw = format!("[{}]", items.join(","));
        let frames = parse_visual_batch_response(&raw, 5).unwrap();
        assert_eq!(frames.len(), 5);
        for (i, f) in frames.iter().enumerate() {
            assert_eq!(f.frame_index, i as i32);
        }
    }

    #[test]
    fn unwraps_a_single_key_results_envelope() {
        let raw = format!(r#"{{"results": [{}]}}"#, valid_frame_json(0, "safe", 0));
        let frames = parse_visual_batch_response(&raw, 1).unwrap();
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn wraps_a_bare_object_when_expected_count_is_one() {
        let raw = valid_frame_json(0, "safe", 0); // bare object, not an array
        let frames = parse_visual_batch_response(&raw, 1).unwrap();
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn rejects_non_json_response() {
        let result = parse_visual_batch_response("not json at all", 1);
        assert_eq!(result.unwrap_err(), ModelOutputError::InvalidJson);
    }

    #[test]
    fn rejects_a_second_independent_json_document_as_ambiguous() {
        // The whole raw response contains two independent top-level JSON documents back
        // to back: a valid single-frame array, then a second, separate object. This must
        // be rejected as ambiguous by `extract_single_json_document` itself -- not "parse
        // the first document and ignore the trailing garbage". (Wrapping the concatenation
        // in an outer `[...]` would instead produce a plain array-syntax error before ever
        // reaching the ambiguity check, which doesn't exercise the behavior this test needs.)
        let first_document = format!("[{}]", valid_frame_json(0, "safe", 0));
        let raw = format!("{first_document}{}", r#"{"unexpected": true}"#);
        let result = parse_visual_batch_response(&raw, 1);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_wrong_array_length() {
        let raw = format!("[{}]", valid_frame_json(0, "safe", 0));
        let result = parse_visual_batch_response(&raw, 5);
        assert_eq!(result.unwrap_err(), ModelOutputError::InvalidSchema);
    }

    #[test]
    fn rejects_frame_index_not_matching_list_position() {
        let raw = format!("[{}]", valid_frame_json(7, "safe", 0)); // index 7 at position 0
        let result = parse_visual_batch_response(&raw, 1);
        assert_eq!(result.unwrap_err(), ModelOutputError::InvalidSchema);
    }

    #[test]
    fn parses_a_text_moderation_response() {
        let raw = r#"{"top_category": "safe", "reason": "clean text",
            "categories": {"safe":0,"suggestive":0,"nudity":0,"porn":0,"gore":0,"violence":0,
            "self_harm":0,"hate_or_extremism":0,"drugs":0,"unknown":0,"sexual_minor_content":0}}"#;
        let output = parse_text_moderation_response(raw).unwrap();
        assert_eq!(output.top_category, "safe");
    }

    #[test]
    fn unwraps_a_moderation_envelope_key_for_text() {
        let raw = r#"{"moderation": {"top_category": "safe", "reason": "clean",
            "categories": {"safe":0,"suggestive":0,"nudity":0,"porn":0,"gore":0,"violence":0,
            "self_harm":0,"hate_or_extremism":0,"drugs":0,"unknown":0,"sexual_minor_content":0}}}"#;
        let output = parse_text_moderation_response(raw).unwrap();
        assert_eq!(output.top_category, "safe");
    }
}
