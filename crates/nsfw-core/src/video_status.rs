use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoJobStatus {
    Queued,
    Processing,
    Classified,
    FailedRetryable,
    FailedTerminal,
    /// Declared for wire/API compatibility; no Python code path assigns this status today.
    Superseded,
}

impl VideoJobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Classified | Self::FailedTerminal | Self::Superseded
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_expected_snake_case_wire_values() {
        assert_eq!(
            serde_json::to_string(&VideoJobStatus::Queued).unwrap(),
            "\"queued\""
        );
        assert_eq!(
            serde_json::to_string(&VideoJobStatus::Processing).unwrap(),
            "\"processing\""
        );
        assert_eq!(
            serde_json::to_string(&VideoJobStatus::Classified).unwrap(),
            "\"classified\""
        );
        assert_eq!(
            serde_json::to_string(&VideoJobStatus::FailedRetryable).unwrap(),
            "\"failed_retryable\""
        );
        assert_eq!(
            serde_json::to_string(&VideoJobStatus::FailedTerminal).unwrap(),
            "\"failed_terminal\""
        );
        assert_eq!(
            serde_json::to_string(&VideoJobStatus::Superseded).unwrap(),
            "\"superseded\""
        );
    }

    #[test]
    fn only_classified_failed_terminal_and_superseded_are_terminal() {
        assert!(!VideoJobStatus::Queued.is_terminal());
        assert!(!VideoJobStatus::Processing.is_terminal());
        assert!(VideoJobStatus::Classified.is_terminal());
        // FailedRetryable is deliberately NOT terminal -- this is the basis of the
        // FAILED_RETRYABLE re-enqueue quirk documented in spec §5, preserved on purpose.
        assert!(!VideoJobStatus::FailedRetryable.is_terminal());
        assert!(VideoJobStatus::FailedTerminal.is_terminal());
        assert!(VideoJobStatus::Superseded.is_terminal());
    }
}
