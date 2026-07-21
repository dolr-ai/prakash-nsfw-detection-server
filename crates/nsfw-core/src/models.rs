use crate::video_status::VideoJobStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameModerationResult {
    pub frame_index: i32,
    pub frame_timestamp_seconds: f64,
    pub top_category: String,
    pub is_nsfw: bool,
    pub overall_severity: u8,
    pub categories: HashMap<String, u8>,
    pub reason: String,
    /// Full parsed model output, including its own computed fields.
    pub raw_response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAction {
    pub action_id: String,
    pub job_id: String,
    pub video_id: String,
    pub publisher_user_id: String,
    pub action_type: String,
    pub threshold: f64,
    pub final_score: f64,
    pub request_url: String,
    pub request_body: serde_json::Value,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJob {
    pub job_id: String,
    pub video_id: String,
    pub source_object_version: String,
    pub policy_version: String,
    pub status: VideoJobStatus,
    pub publisher_user_id: String,
    pub post_id: Option<String>,
    pub canister_id: Option<String>,
    pub source_video_uri: String,
    pub upload_event_id: Option<String>,
    pub trace_id: Option<String>,
    pub attempts: i32,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Kept as a struct even though nothing currently persists it beyond
/// duration_seconds/frames_extracted (spec §17 item 3) -- width/height/fps/codec_name/
/// has_video_stream are computed by ffprobe parsing but discarded in the source today.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub job_id: String,
    pub video_id: String,
    pub duration_seconds: f64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec_name: Option<String>,
    pub has_video_stream: bool,
    pub frames_extracted: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoModerationResult {
    pub job_id: String,
    pub video_id: String,
    pub policy_version: String,
    pub prompt_version: String,
    pub aggregation_version: String,
    pub final_is_nsfw: bool,
    pub final_score: f64,
    pub final_top_category: String,
    pub max_overall_severity: u8,
    pub nsfw_frame_count: i32,
    pub total_frame_count: i32,
    pub move_required: bool,
    pub move_threshold: f64,
    pub max_category_severities: HashMap<String, u8>,
    pub legacy_nsfw_ec: String,
    pub legacy_nsfw_gore: String,
    pub final_response: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    #[test]
    fn frame_moderation_result_round_trips_through_json() {
        let original = FrameModerationResult {
            frame_index: 2,
            frame_timestamp_seconds: 2.0,
            top_category: "safe".to_string(),
            is_nsfw: false,
            overall_severity: 0,
            categories: HashMap::new(),
            reason: "nothing unsafe visible".to_string(),
            raw_response: serde_json::json!({"frame_index": 2}),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: FrameModerationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.frame_index, 2);
        assert_eq!(parsed.top_category, "safe");
    }

    #[test]
    fn video_job_defaults_optional_fields_to_none() {
        let job = VideoJob {
            job_id: "nsfw:v1:policy:etag".to_string(),
            video_id: "v1".to_string(),
            source_object_version: "etag".to_string(),
            policy_version: "nsfw_policy_v1".to_string(),
            status: crate::video_status::VideoJobStatus::Queued,
            publisher_user_id: "user-1".to_string(),
            post_id: None,
            canister_id: None,
            source_video_uri: "https://example.com/v.mp4".to_string(),
            upload_event_id: None,
            trace_id: None,
            attempts: 0,
            last_error_code: None,
            last_error_message: None,
            created_at: None,
            updated_at: None,
            started_at: None,
            finished_at: None,
        };
        assert_eq!(job.attempts, 0);
        assert!(job.post_id.is_none());
    }

    #[test]
    fn video_moderation_result_round_trips_through_json() {
        let result = VideoModerationResult {
            job_id: "job-1".to_string(),
            video_id: "v1".to_string(),
            policy_version: "nsfw_policy_v1".to_string(),
            prompt_version: "visual_batch_moderation_v1".to_string(),
            aggregation_version: "hard_any_frame_v1".to_string(),
            final_is_nsfw: true,
            final_score: 0.8,
            final_top_category: "porn".to_string(),
            max_overall_severity: 4,
            nsfw_frame_count: 1,
            total_frame_count: 3,
            move_required: true,
            move_threshold: 0.8,
            max_category_severities: HashMap::new(),
            legacy_nsfw_ec: "explicit".to_string(),
            legacy_nsfw_gore: "VERY_UNLIKELY".to_string(),
            final_response: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: VideoModerationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.final_top_category, "porn");
        assert!(parsed.move_required);
    }
}
