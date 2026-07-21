# Rust NSFW Port — Phase 1: Workspace, Core Domain Logic & Static Config — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Cargo workspace and implement the pure, zero-I/O domain logic (moderation thresholds, aggregation, legacy mapping, error types, model-response parsing) plus the static env-loaded `Settings` struct — Phase 1 of the 10-phase plan in the approved design spec.

**Architecture:** Cargo workspace with one crate per Phase-1 deliverable: `nsfw-core` (domain models, moderation policy, legacy mapping, aggregation, error types, model-output parsing — no I/O, no async) and `nsfw-config` (env-loaded `Settings`, testable via an in-memory map rather than real process env). Later phases add `nsfw-repositories`, `nsfw-clients`, `nsfw-services`, and the `nsfw-api`/`nsfw-video-worker`/`nsfw-flush-worker` binaries — not part of this plan.

**Tech Stack:** Rust 1.95, edition 2024, `serde`/`serde_json`, `chrono`, `thiserror`, `http` (for `StatusCode`), `secrecy` (secret redaction), `rstest` (parameterized tests).

**Spec:** `docs/superpowers/specs/2026-07-21-rust-nsfw-detection-port-design.md` (approved, 6 review rounds). **Audit reference:** `docs/superpowers/specs/2026-07-21-python-service-source-audit.md`. Every business-rule constant in this plan is transcribed from those two documents — do not re-derive from the Python source's own `plan.md`, which is known to have drifted from the actual implementation (see spec §2, §5).

**Naming note (minor refinement from the spec's illustrative tree):** the spec's architecture diagram (§3) uses the crate name `core`. This plan uses `nsfw-core` instead — a bare crate named `core` risks colliding with Rust's own sysroot `core` crate name in ways that are easy to get wrong. Same purpose, same contents, safer name. All other crates get an `nsfw-` prefix for consistency.

---

## File Structure

```
Cargo.toml                          # workspace root — no [package], members + shared [workspace.dependencies]
crates/
  nsfw-core/
    Cargo.toml
    src/
      lib.rs                        # re-exports every module below
      moderation.rs                 # MODERATION_CATEGORIES, CATEGORY_BLOCK_THRESHOLDS, RISK_ORDER,
                                     #   compute_is_nsfw, compute_overall_severity
      video_status.rs               # VideoJobStatus enum, is_terminal()
      legacy_mapping.rs             # map_legacy_nsfw_ec, map_legacy_nsfw_gore
      aggregation.rs                # aggregate(), top-category tie-break
      models.rs                     # FrameModerationResult, StorageAction, VideoJob,
                                     #   VideoMetadata, VideoModerationResult
      error.rs                      # ErrorCode, AppError
      model_output.rs               # ModerationModelOutput, FrameModerationOutput, TextModerationOutput,
                                     #   parse_visual_batch_response, parse_text_moderation_response
  nsfw-config/
    Cargo.toml
    src/
      lib.rs
      settings.rs                   # Settings, ConfigError, Settings::from_env/from_map, helper methods
.github/
  workflows/
    ci.yml                          # fmt --check, clippy -D warnings, test --workspace
```

Existing `Cargo.toml` (single-package "Hello, world!" scaffold) and `src/main.rs` at the repo root are replaced entirely by the workspace root `Cargo.toml` in Task 1 — neither file has been committed yet (confirmed via `git status`), so this is a clean rewrite, not a migration.

---

### Task 1: Workspace scaffold + CI

**Files:**
- Modify: `Cargo.toml` (replace single-package manifest with workspace manifest)
- Delete: `src/main.rs`
- Create: `crates/nsfw-core/Cargo.toml`, `crates/nsfw-core/src/lib.rs`
- Create: `crates/nsfw-config/Cargo.toml`, `crates/nsfw-config/src/lib.rs`
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Replace the root `Cargo.toml` with a workspace manifest**

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2024"
version = "0.1.0"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
http = "1"
secrecy = { version = "0.10", features = ["serde"] }
rstest = "0.23"
```

- [ ] **Step 2: Remove the old single-package scaffold**

```bash
rm src/main.rs
rmdir src 2>/dev/null || true
```

- [ ] **Step 3: Create `crates/nsfw-core/Cargo.toml`**

```toml
[package]
name = "nsfw-core"
edition.workspace = true
version.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
http = { workspace = true }

[dev-dependencies]
rstest = { workspace = true }
```

- [ ] **Step 4: Create an empty `crates/nsfw-core/src/lib.rs`**

```rust
// Modules added task-by-task in this plan.
```

- [ ] **Step 5: Create `crates/nsfw-config/Cargo.toml`**

```toml
[package]
name = "nsfw-config"
edition.workspace = true
version.workspace = true

[dependencies]
secrecy = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 6: Create an empty `crates/nsfw-config/src/lib.rs`**

```rust
// Modules added task-by-task in this plan.
```

- [ ] **Step 7: Verify the workspace builds**

Run: `cargo build --workspace`
Expected: builds successfully, two empty library crates.

- [ ] **Step 8: Add CI workflow**

```yaml
name: CI

on:
  push:
    branches: [main, master]
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ .github/ .gitignore
git rm --cached src/main.rs 2>/dev/null || true
git commit -m "chore: convert to cargo workspace, scaffold nsfw-core and nsfw-config crates"
```

---

### Task 2: Moderation category constants & threshold logic

**Files:**
- Create: `crates/nsfw-core/src/moderation.rs`
- Modify: `crates/nsfw-core/src/lib.rs`

This is the single most consequential parity requirement in the whole port (spec §5, §7.1): Python's actual `compute_is_nsfw` uses **per-category thresholds**, not the `top_category in unsafe_categories OR overall_severity >= 3` rule that the Python repo's own pre-implementation `plan.md` describes. Get this table exactly right — it's confirmed against the Python repo's own `tests/unit/services/test_moderation_policy.py`.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/moderation.rs
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
    fn compute_is_nsfw_matches_category_thresholds(#[case] input: &[(&str, u8)], #[case] expected: bool) {
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
        assert!(!thresholded.contains(&"safe"), "safe must never have a block threshold");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib moderation`
Expected: FAIL — `compute_is_nsfw`, `compute_overall_severity`, `CATEGORY_BLOCK_THRESHOLDS`, `MODERATION_CATEGORIES` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/moderation.rs (above the #[cfg(test)] module)
use std::collections::HashMap;

pub const MODERATION_CATEGORIES: [&str; 11] = [
    "safe", "suggestive", "nudity", "porn", "gore", "violence",
    "self_harm", "hate_or_extremism", "drugs", "unknown", "sexual_minor_content",
];

/// Ground truth is the actual Python code, not the Python repo's own `plan.md`
/// (which describes a different, unimplemented rule). See design spec §5/§7.1.
pub const CATEGORY_BLOCK_THRESHOLDS: &[(&str, u8)] = &[
    ("sexual_minor_content", 3),
    ("porn", 4), ("gore", 4), ("violence", 4), ("self_harm", 4),
    ("hate_or_extremism", 4), ("drugs", 4), ("unknown", 4),
    ("suggestive", 5), ("nudity", 5),
    // "safe" is deliberately absent — never triggers is_nsfw
];

pub const RISK_ORDER: [&str; 11] = [
    "sexual_minor_content", "porn", "nudity", "gore", "violence",
    "self_harm", "hate_or_extremism", "drugs", "suggestive", "unknown", "safe",
];

pub fn compute_is_nsfw(categories: &HashMap<String, u8>) -> bool {
    CATEGORY_BLOCK_THRESHOLDS
        .iter()
        .any(|(cat, threshold)| categories.get(*cat).copied().unwrap_or(0) >= *threshold)
}

pub fn compute_overall_severity(top_category: &str, categories: &HashMap<String, u8>) -> Option<u8> {
    if !MODERATION_CATEGORIES.contains(&top_category) {
        return None;
    }
    Some(categories.get(top_category).copied().unwrap_or(0))
}
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-core/src/lib.rs
pub mod moderation;
pub use moderation::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib moderation`
Expected: PASS (7 tests: 6 rstest cases + 3 plain tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-core/src/moderation.rs crates/nsfw-core/src/lib.rs
git commit -m "feat: port moderation category thresholds and is_nsfw logic"
```

---

### Task 3: Video job status enum

**Files:**
- Create: `crates/nsfw-core/src/video_status.rs`
- Modify: `crates/nsfw-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/video_status.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_expected_snake_case_wire_values() {
        assert_eq!(serde_json::to_string(&VideoJobStatus::Queued).unwrap(), "\"queued\"");
        assert_eq!(serde_json::to_string(&VideoJobStatus::Processing).unwrap(), "\"processing\"");
        assert_eq!(serde_json::to_string(&VideoJobStatus::Classified).unwrap(), "\"classified\"");
        assert_eq!(serde_json::to_string(&VideoJobStatus::FailedRetryable).unwrap(), "\"failed_retryable\"");
        assert_eq!(serde_json::to_string(&VideoJobStatus::FailedTerminal).unwrap(), "\"failed_terminal\"");
        assert_eq!(serde_json::to_string(&VideoJobStatus::Superseded).unwrap(), "\"superseded\"");
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib video_status`
Expected: FAIL — `VideoJobStatus` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/video_status.rs (above the #[cfg(test)] module)
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
        matches!(self, Self::Classified | Self::FailedTerminal | Self::Superseded)
    }
}
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-core/src/lib.rs (append)
pub mod video_status;
pub use video_status::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib video_status`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-core/src/video_status.rs crates/nsfw-core/src/lib.rs
git commit -m "feat: port VideoJobStatus enum and terminal-status classification"
```

---

### Task 4: Legacy mapping functions

**Files:**
- Create: `crates/nsfw-core/src/legacy_mapping.rs`
- Modify: `crates/nsfw-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/legacy_mapping.rs
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
    fn maps_max_gore_violence_severity_to_legacy_nsfw_gore(#[case] input: &[(&str, u8)], #[case] expected: &str) {
        assert_eq!(map_legacy_nsfw_gore(&severities(input)), expected);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib legacy_mapping`
Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/legacy_mapping.rs (above the #[cfg(test)] module)
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
    let violence = max_category_severities.get("violence").copied().unwrap_or(0);
    match gore.max(violence) {
        s if s >= 5 => "VERY_LIKELY",
        s if s >= 4 => "LIKELY",
        s if s >= 3 => "POSSIBLE",
        s if s >= 1 => "UNLIKELY",
        _ => "VERY_UNLIKELY",
    }
}
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-core/src/lib.rs (append)
pub mod legacy_mapping;
pub use legacy_mapping::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib legacy_mapping`
Expected: PASS (13 tests: 6 + 7 rstest cases).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-core/src/legacy_mapping.rs crates/nsfw-core/src/lib.rs
git commit -m "feat: port legacy nsfw_ec/nsfw_gore mapping functions"
```

---

### Task 5: Aggregation logic

**Files:**
- Create: `crates/nsfw-core/src/aggregation.rs`
- Modify: `crates/nsfw-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/aggregation.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn frame(top_category: &str, overall_severity: u8, cats: &[(&str, u8)], is_nsfw: bool) -> AggregationInputFrame {
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
        assert!(out.final_is_nsfw, "one bad frame must flag the whole video -- no averaging");
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
        let frames = vec![frame("sexual_minor_content", 3, &[("sexual_minor_content", 3)], true)];
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
            frame("sexual_minor_content", 5, &[("sexual_minor_content", 5)], true),
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib aggregation`
Expected: FAIL — `aggregate`, `AggregationInputFrame`, `AggregationError` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/aggregation.rs (above the #[cfg(test)] module)
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
pub fn aggregate(frames: &[AggregationInputFrame], move_threshold: f64) -> Result<AggregationOutput, AggregationError> {
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
    RISK_ORDER.iter().position(|c| *c == category).unwrap_or(RISK_ORDER.len())
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
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-core/src/lib.rs (append)
pub mod aggregation;
pub use aggregation::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib aggregation`
Expected: PASS (9 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-core/src/aggregation.rs crates/nsfw-core/src/lib.rs
git commit -m "feat: port video aggregation policy and top-category tie-break"
```

---

### Task 6: Domain structs

**Files:**
- Create: `crates/nsfw-core/src/models.rs`
- Modify: `crates/nsfw-core/src/lib.rs`

Field-for-field ports of the Python dataclasses (audit §10 / spec §7.2). Mostly plain data — the test here is a construction + JSON round-trip sanity check per struct, not business logic (there is none here).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/models.rs
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib models`
Expected: FAIL — structs not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/models.rs (above the #[cfg(test)] module)
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
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-core/src/lib.rs (append)
pub mod models;
pub use models::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib models`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-core/src/models.rs crates/nsfw-core/src/lib.rs
git commit -m "feat: port domain structs (VideoJob, FrameModerationResult, VideoModerationResult, etc.)"
```

---

### Task 7: Error handling — `ErrorCode` and `AppError`

**Files:**
- Create: `crates/nsfw-core/src/error.rs`
- Modify: `crates/nsfw-core/src/lib.rs`

Every code and its status is a fixed 1:1 mapping (spec §7.3's "Complete error code table") — there is no per-call-site status override needed anywhere in this table.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/error.rs
#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[rstest::rstest]
    #[case(ErrorCode::AuthMissingHeaders, "auth_missing_headers", StatusCode::UNAUTHORIZED)]
    #[case(ErrorCode::AuthBadTimestamp, "auth_bad_timestamp", StatusCode::UNAUTHORIZED)]
    #[case(ErrorCode::AuthTimestampOutOfRange, "auth_timestamp_out_of_range", StatusCode::UNAUTHORIZED)]
    #[case(ErrorCode::AuthBadSignature, "auth_bad_signature", StatusCode::UNAUTHORIZED)]
    #[case(ErrorCode::NotFound, "not_found", StatusCode::NOT_FOUND)]
    #[case(ErrorCode::ServiceUnavailable, "service_unavailable", StatusCode::SERVICE_UNAVAILABLE)]
    #[case(ErrorCode::QueueUnavailable, "queue_unavailable", StatusCode::SERVICE_UNAVAILABLE)]
    #[case(ErrorCode::ValidationError, "validation_error", StatusCode::UNPROCESSABLE_ENTITY)]
    #[case(ErrorCode::ModelModerationFailed, "model_moderation_failed", StatusCode::SERVICE_UNAVAILABLE)]
    #[case(ErrorCode::ModelResponseInvalidJson, "model_response_invalid_json", StatusCode::BAD_GATEWAY)]
    #[case(ErrorCode::ModelResponseInvalidSchema, "model_response_invalid_schema", StatusCode::BAD_GATEWAY)]
    #[case(ErrorCode::ImageDownloadFailed, "image_download_failed", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::ImageDownloadTimeout, "image_download_timeout", StatusCode::GATEWAY_TIMEOUT)]
    #[case(ErrorCode::ImageDownloadUpstreamError, "image_download_upstream_error", StatusCode::BAD_GATEWAY)]
    #[case(ErrorCode::VideoDownloadEmpty, "video_download_empty", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::VideoTooLarge, "video_too_large", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::VideoNoStream, "video_no_stream", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::VideoProbeFailed, "video_probe_failed", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::VideoExtractionFailed, "video_extraction_failed", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::GpuNotConfigured, "gpu_not_configured", StatusCode::SERVICE_UNAVAILABLE)]
    #[case(ErrorCode::InvalidImageBase64, "invalid_image_base64", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::EmptyImage, "empty_image", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::ImageTooLarge, "image_too_large", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::StorjNotConfigured, "storj_not_configured", StatusCode::SERVICE_UNAVAILABLE)]
    fn error_code_matches_exact_wire_string_and_status(
        #[case] code: ErrorCode,
        #[case] expected_str: &str,
        #[case] expected_status: StatusCode,
    ) {
        assert_eq!(code.as_str(), expected_str);
        assert_eq!(code.default_status(), expected_status);
    }

    #[test]
    fn declared_but_never_raised_codes_still_exist_for_registry_completeness() {
        // Python declares these in codes.py but never raises them anywhere -- carried
        // here for parity but no production call site should ever construct them.
        assert_eq!(ErrorCode::NotImplemented.as_str(), "not_implemented");
        assert_eq!(ErrorCode::QueueError.as_str(), "queue_error");
    }

    #[test]
    fn app_error_new_applies_the_codes_default_status() {
        let err = AppError::new(ErrorCode::NotFound, "video job not found");
        assert_eq!(err.code.as_str(), "not_found");
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.message, "video job not found");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib error`
Expected: FAIL — `ErrorCode`, `AppError` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/error.rs (above the #[cfg(test)] module)
use http::StatusCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    AuthMissingHeaders,
    AuthBadTimestamp,
    AuthTimestampOutOfRange,
    AuthBadSignature,
    NotFound,
    ServiceUnavailable,
    QueueUnavailable,
    ValidationError,
    ModelModerationFailed,
    ModelResponseInvalidJson,
    ModelResponseInvalidSchema,
    ImageDownloadFailed,
    ImageDownloadTimeout,
    ImageDownloadUpstreamError,
    VideoDownloadEmpty,
    VideoTooLarge,
    VideoNoStream,
    VideoProbeFailed,
    VideoExtractionFailed,
    GpuNotConfigured,
    InvalidImageBase64,
    EmptyImage,
    ImageTooLarge,
    StorjNotConfigured,
    /// Declared in Python's codes.py, never raised. Carried for parity; never construct this.
    NotImplemented,
    /// Declared in Python's codes.py, never raised. Carried for parity; never construct this.
    QueueError,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthMissingHeaders => "auth_missing_headers",
            Self::AuthBadTimestamp => "auth_bad_timestamp",
            Self::AuthTimestampOutOfRange => "auth_timestamp_out_of_range",
            Self::AuthBadSignature => "auth_bad_signature",
            Self::NotFound => "not_found",
            Self::ServiceUnavailable => "service_unavailable",
            Self::QueueUnavailable => "queue_unavailable",
            Self::ValidationError => "validation_error",
            Self::ModelModerationFailed => "model_moderation_failed",
            Self::ModelResponseInvalidJson => "model_response_invalid_json",
            Self::ModelResponseInvalidSchema => "model_response_invalid_schema",
            Self::ImageDownloadFailed => "image_download_failed",
            Self::ImageDownloadTimeout => "image_download_timeout",
            Self::ImageDownloadUpstreamError => "image_download_upstream_error",
            Self::VideoDownloadEmpty => "video_download_empty",
            Self::VideoTooLarge => "video_too_large",
            Self::VideoNoStream => "video_no_stream",
            Self::VideoProbeFailed => "video_probe_failed",
            Self::VideoExtractionFailed => "video_extraction_failed",
            Self::GpuNotConfigured => "gpu_not_configured",
            Self::InvalidImageBase64 => "invalid_image_base64",
            Self::EmptyImage => "empty_image",
            Self::ImageTooLarge => "image_too_large",
            Self::StorjNotConfigured => "storj_not_configured",
            Self::NotImplemented => "not_implemented",
            Self::QueueError => "queue_error",
        }
    }

    pub fn default_status(&self) -> StatusCode {
        match self {
            Self::AuthMissingHeaders
            | Self::AuthBadTimestamp
            | Self::AuthTimestampOutOfRange
            | Self::AuthBadSignature => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ServiceUnavailable
            | Self::QueueUnavailable
            | Self::ModelModerationFailed
            | Self::GpuNotConfigured
            | Self::StorjNotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            Self::ValidationError => StatusCode::UNPROCESSABLE_ENTITY,
            Self::ModelResponseInvalidJson
            | Self::ModelResponseInvalidSchema
            | Self::ImageDownloadUpstreamError => StatusCode::BAD_GATEWAY,
            Self::ImageDownloadTimeout => StatusCode::GATEWAY_TIMEOUT,
            Self::ImageDownloadFailed
            | Self::VideoDownloadEmpty
            | Self::VideoTooLarge
            | Self::VideoNoStream
            | Self::VideoProbeFailed
            | Self::VideoExtractionFailed
            | Self::InvalidImageBase64
            | Self::EmptyImage
            | Self::ImageTooLarge => StatusCode::BAD_REQUEST,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::QueueError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub status: StatusCode,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let status = code.default_status();
        Self { code, message: message.into(), status }
    }
}
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-core/src/lib.rs (append)
pub mod error;
pub use error::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib error`
Expected: PASS (25 tests: 23 rstest cases + 2 plain tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-core/src/error.rs crates/nsfw-core/src/lib.rs
git commit -m "feat: port AppError and the complete error code table"
```

---

### Task 8: Model-response validation — `ModerationModelOutput`

**Files:**
- Create: `crates/nsfw-core/src/model_output.rs`
- Modify: `crates/nsfw-core/src/lib.rs`

This is `app/schemas/model_output.py`'s validators (audit §2): every category must be present with a `0..=5` score, and `top_category`'s own score must equal the max unsafe severity (or, if `top_category == "safe"`, every unsafe category must be `0`).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/model_output.rs
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
        let output = ModerationModelOutput::parse("safe".to_string(), cats, "nothing visible".to_string());
        assert!(output.is_ok());
        let output = output.unwrap();
        assert!(!output.is_nsfw);
        assert_eq!(output.overall_severity, 0);
    }

    #[test]
    fn accepts_top_category_matching_the_max_unsafe_severity() {
        let cats = full_categories(&[("porn", 4), ("gore", 2)]);
        let output = ModerationModelOutput::parse("porn".to_string(), cats, "explicit content".to_string()).unwrap();
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
        let result = ModerationModelOutput::parse("not_a_category".to_string(), cats, "x".to_string());
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib model_output`
Expected: FAIL — `ModerationModelOutput` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/model_output.rs (above the #[cfg(test)] module)
use crate::moderation::{compute_is_nsfw, MODERATION_CATEGORIES};
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
        Ok(Self { top_category, categories, reason, overall_severity, is_nsfw })
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
        return if max_unsafe == 0 { Ok(()) } else { Err(ModelOutputError::InvalidSchema) };
    }

    let top_score = categories.get(top_category).copied().unwrap_or(0);
    if top_score == 0 || top_score != max_unsafe {
        return Err(ModelOutputError::InvalidSchema);
    }
    Ok(())
}
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-core/src/lib.rs (append)
pub mod model_output;
pub use model_output::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib model_output`
Expected: PASS (7 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-core/src/model_output.rs crates/nsfw-core/src/lib.rs
git commit -m "feat: port ModerationModelOutput validation (category/top_category consistency)"
```

---

### Task 9: Model-response JSON parsing — batch and text

**Files:**
- Modify: `crates/nsfw-core/src/model_output.rs` (add `FrameModerationOutput`, `TextModerationOutput`, `parse_visual_batch_response`, `parse_text_moderation_response`)

Replicates `parse_visual_batch_response`/`parse_text_moderation_response` (audit §2): tolerant single-JSON-document extraction (reject if a second independent document follows — that's "ambiguous"), envelope-key unwrapping, array-length-must-equal-expected-count, and `frame_index` must equal list position.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-core/src/model_output.rs (append to the #[cfg(test)] module)
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
        let raw = format!("{}{}", valid_frame_json(0, "safe", 0), r#"{"unexpected": true}"#);
        // Two independent top-level values in the same string -- ambiguous, not "take the first one".
        let result = parse_visual_batch_response(&format!("[{raw}]"), 1);
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-core --lib model_output::batch_and_text_parsing_tests`
Expected: FAIL — `FrameModerationOutput`, `parse_visual_batch_response`, `parse_text_moderation_response` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-core/src/model_output.rs (append, above the test modules)
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
    if let serde_json::Value::Object(ref map) = value {
        if map.len() == 1 {
            if let Some((key, inner)) = map.iter().next() {
                if keys.contains(&key.as_str()) {
                    return inner.clone();
                }
            }
        }
    }
    value
}

fn parse_categories_object(value: Option<&serde_json::Value>) -> Result<HashMap<String, u8>, ModelOutputError> {
    let obj = value.and_then(|v| v.as_object()).ok_or(ModelOutputError::InvalidSchema)?;
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

fn parse_frame_item(item: serde_json::Value, position: usize) -> Result<FrameModerationOutput, ModelOutputError> {
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
    let reason = obj.get("reason").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let categories = parse_categories_object(obj.get("categories"))?;

    let base = ModerationModelOutput::parse(top_category, categories, reason)?;
    Ok(FrameModerationOutput { base, frame_index })
}

pub fn parse_text_moderation_response(raw_response: &str) -> Result<TextModerationOutput, ModelOutputError> {
    let value = extract_single_json_document(raw_response)?;
    let value = unwrap_envelope(value, &["result", "moderation"]);
    let obj = value.as_object().ok_or(ModelOutputError::InvalidSchema)?;

    let top_category = obj
        .get("top_category")
        .and_then(|v| v.as_str())
        .ok_or(ModelOutputError::InvalidSchema)?
        .to_string();
    let reason = obj.get("reason").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let categories = parse_categories_object(obj.get("categories"))?;

    ModerationModelOutput::parse(top_category, categories, reason)
}
```

**Note:** `FrameModerationOutput` field access in tests above uses `frames[0].base.overall_severity` — the `base: ModerationModelOutput` field must be `pub` (it is, per Step 3).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p nsfw-core --lib model_output`
Expected: PASS (all tests in both `model_output` test modules — 17 total).

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-core/src/model_output.rs
git commit -m "feat: port visual-batch and text model-response JSON parsing"
```

---

### Task 10: `nsfw-config` — static `Settings`

**Files:**
- Create: `crates/nsfw-config/src/settings.rs`
- Modify: `crates/nsfw-config/src/lib.rs`

Transcribed field-for-field from the audit's §1 table. Two quirks that are easy to silently drop (call these out in code comments, not just the plan): the `API_BASE_URL `/`API_KEY `/`MODEL_NAME ` trailing-space aliases, and secret redaction in `Debug` output.

**Design choice for testability:** `Settings::from_env()` is a thin wrapper around `Settings::from_map(&HashMap<String, String>)`. Tests build a map directly instead of mutating real process env vars — this avoids test flakiness from parallel test execution racing on shared process env state, with zero behavior difference in production (`from_env` just collects `std::env::vars()` into a map and calls `from_map`).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-config/src/settings.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn defaults_apply_when_env_is_empty() {
        let settings = Settings::from_map(&HashMap::new()).unwrap();
        assert_eq!(settings.app_name, "yral-nsfw-detector");
        assert_eq!(settings.environment, "local");
        assert_eq!(settings.internal_request_max_skew_sec, 300);
        assert_eq!(settings.kvrocks_port, 6379);
        assert!(settings.kvrocks_cluster_enabled);
        assert_eq!(settings.move_threshold, 0.8);
        assert_eq!(settings.frame_batch_size, 5);
        assert_eq!(settings.gpu_max_concurrency, 5);
        assert_eq!(settings.gpu_max_attempts, 3);
        assert_eq!(settings.gpu_retry_base_delay_seconds, 0.25);
        assert_eq!(settings.video_max_bytes, 512 * 1024 * 1024);
        assert_eq!(settings.image_max_bytes, 10 * 1024 * 1024);
        assert_eq!(settings.queue_stream_name, "nsfw:queue:video_detection");
        assert_eq!(settings.runtime_nsfw_key_prefix, "offchain:video_nsfw:");
    }

    #[test]
    fn reads_explicit_env_values_over_defaults() {
        let settings = Settings::from_map(&map(&[
            ("KVROCKS_PORT", "7000"),
            ("MOVE_THRESHOLD_UNUSED_PLACEHOLDER", "ignored"), // sanity: unknown keys are harmless
        ]))
        .unwrap();
        assert_eq!(settings.kvrocks_port, 7000);
    }

    #[test]
    fn api_base_url_accepts_the_trailing_space_legacy_alias() {
        // Historical .env typo compat -- must keep working or prod config silently breaks on cutover.
        let settings = Settings::from_map(&map(&[("API_BASE_URL ", "https://gpu.example.com")])).unwrap();
        assert_eq!(settings.api_base_url.as_deref(), Some("https://gpu.example.com"));
    }

    #[test]
    fn api_base_url_without_trailing_space_still_works() {
        let settings = Settings::from_map(&map(&[("API_BASE_URL", "https://gpu.example.com")])).unwrap();
        assert_eq!(settings.api_base_url.as_deref(), Some("https://gpu.example.com"));
    }

    #[test]
    fn invalid_bool_value_is_a_config_error_not_a_silent_default() {
        let result = Settings::from_map(&map(&[("KVROCKS_TLS_ENABLED", "not-a-bool")]));
        assert!(result.is_err());
    }

    #[test]
    fn is_gpu_configured_requires_all_three_gpu_fields() {
        let none = Settings::from_map(&HashMap::new()).unwrap();
        assert!(!none.is_gpu_configured());

        let partial = Settings::from_map(&map(&[("API_BASE_URL", "https://x"), ("API_KEY", "k")])).unwrap();
        assert!(!partial.is_gpu_configured(), "model_name still missing");

        let full = Settings::from_map(&map(&[
            ("API_BASE_URL", "https://x"),
            ("API_KEY", "k"),
            ("MODEL_NAME", "m"),
        ]))
        .unwrap();
        assert!(full.is_gpu_configured());
    }

    #[test]
    fn is_kvrocks_configured_requires_host() {
        let none = Settings::from_map(&HashMap::new()).unwrap();
        assert!(!none.is_kvrocks_configured());
        let with_host = Settings::from_map(&map(&[("KVROCKS_HOST", "localhost")])).unwrap();
        assert!(with_host.is_kvrocks_configured());
    }

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let settings = Settings::from_map(&map(&[("INTERNAL_REQUEST_HMAC_SECRET", "super-secret-value")])).unwrap();
        let debug_output = format!("{settings:?}");
        assert!(!debug_output.contains("super-secret-value"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p nsfw-config --lib settings`
Expected: FAIL — `Settings` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-config/src/settings.rs (above the #[cfg(test)] module)
use secrecy::SecretString;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid value for {0}: {1:?}")]
    InvalidValue(String, String),
}

fn get_first<'a>(vars: &'a HashMap<String, String>, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|n| vars.get(*n)).map(|s| s.as_str())
}

fn get_string(vars: &HashMap<String, String>, name: &str, default: &str) -> String {
    vars.get(name).cloned().unwrap_or_else(|| default.to_string())
}

fn get_secret(vars: &HashMap<String, String>, name: &str) -> Option<SecretString> {
    vars.get(name).cloned().map(SecretString::from)
}

fn get_bool(vars: &HashMap<String, String>, name: &str, default: bool) -> Result<bool, ConfigError> {
    match vars.get(name) {
        Some(v) => v
            .parse::<bool>()
            .map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_u16(vars: &HashMap<String, String>, name: &str, default: u16) -> Result<u16, ConfigError> {
    match vars.get(name) {
        Some(v) => v.parse::<u16>().map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_u32(vars: &HashMap<String, String>, name: &str, default: u32) -> Result<u32, ConfigError> {
    match vars.get(name) {
        Some(v) => v.parse::<u32>().map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_i64(vars: &HashMap<String, String>, name: &str, default: i64) -> Result<i64, ConfigError> {
    match vars.get(name) {
        Some(v) => v.parse::<i64>().map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_f64(vars: &HashMap<String, String>, name: &str, default: f64) -> Result<f64, ConfigError> {
    match vars.get(name) {
        Some(v) => v.parse::<f64>().map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_u64(vars: &HashMap<String, String>, name: &str, default: u64) -> Result<u64, ConfigError> {
    match vars.get(name) {
        Some(v) => v.parse::<u64>().map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

#[derive(Debug)]
pub struct Settings {
    pub app_name: String,
    pub environment: String,

    pub internal_request_hmac_secret: Option<SecretString>,
    pub internal_request_max_skew_sec: i64,

    pub postgres_database_url: Option<SecretString>,

    pub kvrocks_host: Option<String>,
    pub kvrocks_port: u16,
    pub kvrocks_password: Option<SecretString>,
    pub kvrocks_tls_enabled: bool,
    pub kvrocks_cluster_enabled: bool,
    pub kvrocks_max_connections: u32,
    pub kvrocks_pool_max_attempts: u32,
    pub kvrocks_pool_retry_base_delay_seconds: f64,
    pub kvrocks_socket_timeout_seconds: f64,
    pub kvrocks_socket_connect_timeout_seconds: f64,
    pub kvrocks_health_check_interval_seconds: u32,
    pub kvrocks_ssl_ca_cert: Option<String>,
    pub kvrocks_ssl_client_cert: Option<String>,
    pub kvrocks_ssl_client_key: Option<String>,

    pub clickhouse_primary_database_url: Option<SecretString>,
    /// Declared but never read anywhere in the Python source -- dead config, kept inert here too.
    pub clickhouse_secondary_database_url: Option<SecretString>,
    pub clickhouse_secure: bool,
    pub clickhouse_verify: bool,
    pub clickhouse_database: String,
    pub clickhouse_user: Option<SecretString>,
    pub clickhouse_password: Option<SecretString>,
    pub clickhouse_nsfw_table: String,
    pub clickhouse_nsfw_agg_table: String,
    pub clickhouse_excluded_videos_table: String,
    pub clickhouse_storage_actions_table: String,

    pub storj_interface_url: Option<String>,
    pub storj_interface_token: Option<SecretString>,
    pub storj_interface_timeout_seconds: f64,

    pub api_base_url: Option<String>,
    pub api_key: Option<SecretString>,
    pub model_name: Option<String>,
    pub model_provider: String,
    pub model_version: Option<String>,

    pub sentry_dsn: Option<SecretString>,
    pub sentry_send_default_pii: bool,

    /// Declared but never referenced anywhere else in the Python source -- dead config, kept inert.
    pub default_policy_version: String,
    pub visual_prompt_version: String,
    pub image_prompt_version: String,
    pub image_text_prompt_version: String,
    pub text_prompt_version: String,
    pub aggregation_version: String,

    pub frame_batch_size: u32,
    pub gpu_max_concurrency: u32,
    pub gpu_max_attempts: u32,
    pub gpu_retry_base_delay_seconds: f64,

    pub image_max_bytes: u64,
    pub image_download_timeout_seconds: f64,
    pub image_download_max_attempts: u32,
    pub image_download_retry_base_delay_seconds: f64,

    pub video_download_timeout_seconds: f64,
    pub video_max_bytes: u64,
    pub video_temp_root: String,
    pub ffprobe_timeout_seconds: f64,
    pub ffmpeg_timeout_seconds: f64,

    pub move_threshold: f64,

    pub queue_stream_name: String,
    pub queue_group_name: String,
    pub queue_consumer_name: Option<String>,
    pub queue_read_count: u32,
    pub queue_block_ms: u32,
    pub queue_max_attempts: u32,
    pub queue_dlq_stream_name: String,

    pub clickhouse_buffer_video_results_key: String,
    pub clickhouse_buffer_legacy_key: String,
    pub clickhouse_buffer_storage_actions_key: String,
    pub runtime_nsfw_key_prefix: String,
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_map(&std::env::vars().collect())
    }

    pub fn from_map(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        Ok(Self {
            app_name: get_string(vars, "app_name", "yral-nsfw-detector"),
            environment: get_string(vars, "environment", "local"),

            internal_request_hmac_secret: get_secret(vars, "INTERNAL_REQUEST_HMAC_SECRET"),
            internal_request_max_skew_sec: get_i64(vars, "INTERNAL_REQUEST_MAX_SKEW_SEC", 300)?,

            postgres_database_url: get_secret(vars, "POSTGRES_DATABASE_URL"),

            kvrocks_host: vars.get("KVROCKS_HOST").cloned(),
            kvrocks_port: get_u16(vars, "KVROCKS_PORT", 6379)?,
            kvrocks_password: get_secret(vars, "KVROCKS_PASSWORD"),
            kvrocks_tls_enabled: get_bool(vars, "KVROCKS_TLS_ENABLED", false)?,
            kvrocks_cluster_enabled: get_bool(vars, "KVROCKS_CLUSTER_ENABLED", true)?,
            kvrocks_max_connections: get_u32(vars, "KVROCKS_MAX_CONNECTIONS", 500)?,
            kvrocks_pool_max_attempts: get_u32(vars, "KVROCKS_POOL_MAX_ATTEMPTS", 3)?,
            kvrocks_pool_retry_base_delay_seconds: get_f64(vars, "KVROCKS_POOL_RETRY_BASE_DELAY_SECONDS", 0.05)?,
            kvrocks_socket_timeout_seconds: get_f64(vars, "KVROCKS_SOCKET_TIMEOUT_SECONDS", 5.0)?,
            kvrocks_socket_connect_timeout_seconds: get_f64(vars, "KVROCKS_SOCKET_CONNECT_TIMEOUT_SECONDS", 5.0)?,
            kvrocks_health_check_interval_seconds: get_u32(vars, "KVROCKS_HEALTH_CHECK_INTERVAL_SECONDS", 30)?,
            kvrocks_ssl_ca_cert: vars.get("KVROCKS_SSL_CA_CERT").cloned(),
            kvrocks_ssl_client_cert: vars.get("KVROCKS_SSL_CLIENT_CERT").cloned(),
            kvrocks_ssl_client_key: vars.get("KVROCKS_SSL_CLIENT_KEY").cloned(),

            clickhouse_primary_database_url: get_secret(vars, "CLICKHOUSE_PRIMARY_DATABASE_URL"),
            clickhouse_secondary_database_url: get_secret(vars, "CLICKHOUSE_SECONDARY_DATABASE_URL"),
            clickhouse_secure: get_bool(vars, "CLICKHOUSE_SECURE", true)?,
            clickhouse_verify: get_bool(vars, "CLICKHOUSE_VERIFY", true)?,
            clickhouse_database: get_string(vars, "CLICKHOUSE_DATABASE", "yral"),
            clickhouse_user: get_secret(vars, "CLICKHOUSE_USER"),
            clickhouse_password: get_secret(vars, "CLICKHOUSE_PASSWORD"),
            clickhouse_nsfw_table: get_string(vars, "CLICKHOUSE_NSFW_TABLE", "video_nsfw_detection"),
            clickhouse_nsfw_agg_table: get_string(vars, "CLICKHOUSE_NSFW_AGG_TABLE", "video_nsfw_agg"),
            clickhouse_excluded_videos_table: get_string(vars, "CLICKHOUSE_EXCLUDED_VIDEOS_TABLE", "excluded_videos"),
            clickhouse_storage_actions_table: get_string(
                vars,
                "clickhouse_storage_actions_table",
                "video_nsfw_storage_actions",
            ),

            storj_interface_url: vars.get("STORJ_INTERFACE_URL").cloned(),
            storj_interface_token: get_secret(vars, "STORJ_INTERFACE_TOKEN"),
            storj_interface_timeout_seconds: get_f64(vars, "storj_interface_timeout_seconds", 10.0)?,

            api_base_url: get_first(vars, &["API_BASE_URL", "API_BASE_URL "]).map(str::to_string),
            api_key: get_first(vars, &["API_KEY", "API_KEY "]).map(|v| SecretString::from(v.to_string())),
            model_name: get_first(vars, &["MODEL_NAME", "MODEL_NAME "]).map(str::to_string),
            model_provider: get_string(vars, "model_provider", "openai-compatible"),
            model_version: vars.get("model_version").cloned(),

            sentry_dsn: get_secret(vars, "SENTRY_DSN"),
            sentry_send_default_pii: get_bool(vars, "SENTRY_SEND_DEFAULT_PII", false)?,

            default_policy_version: get_string(vars, "default_policy_version", "nsfw_policy_v1"),
            visual_prompt_version: get_string(vars, "visual_prompt_version", "visual_batch_moderation_v1"),
            image_prompt_version: get_string(vars, "image_prompt_version", "image_generation_moderation_v1"),
            image_text_prompt_version: get_string(
                vars,
                "image_text_prompt_version",
                "image_prompt_generation_moderation_v1",
            ),
            text_prompt_version: get_string(vars, "text_prompt_version", "text_moderation_v1"),
            aggregation_version: get_string(vars, "aggregation_version", "hard_any_frame_v1"),

            frame_batch_size: get_u32(vars, "frame_batch_size", 5)?,
            gpu_max_concurrency: get_u32(vars, "gpu_max_concurrency", 5)?,
            gpu_max_attempts: get_u32(vars, "gpu_max_attempts", 3)?,
            gpu_retry_base_delay_seconds: get_f64(vars, "GPU_RETRY_BASE_DELAY_SECONDS", 0.25)?,

            image_max_bytes: get_u64(vars, "image_max_bytes", 10 * 1024 * 1024)?,
            image_download_timeout_seconds: get_f64(vars, "IMAGE_DOWNLOAD_TIMEOUT_SECONDS", 30.0)?,
            image_download_max_attempts: get_u32(vars, "IMAGE_DOWNLOAD_MAX_ATTEMPTS", 3)?,
            image_download_retry_base_delay_seconds: get_f64(
                vars,
                "IMAGE_DOWNLOAD_RETRY_BASE_DELAY_SECONDS",
                0.5,
            )?,

            video_download_timeout_seconds: get_f64(vars, "video_download_timeout_seconds", 120.0)?,
            video_max_bytes: get_u64(vars, "video_max_bytes", 512 * 1024 * 1024)?,
            video_temp_root: get_string(vars, "video_temp_root", "/tmp/nsfw"),
            ffprobe_timeout_seconds: get_f64(vars, "ffprobe_timeout_seconds", 30.0)?,
            ffmpeg_timeout_seconds: get_f64(vars, "ffmpeg_timeout_seconds", 300.0)?,

            move_threshold: get_f64(vars, "move_threshold", 0.8)?,

            queue_stream_name: get_string(vars, "queue_stream_name", "nsfw:queue:video_detection"),
            queue_group_name: get_string(vars, "queue_group_name", "nsfw_video_workers"),
            queue_consumer_name: vars.get("QUEUE_CONSUMER_NAME").cloned(),
            queue_read_count: get_u32(vars, "QUEUE_READ_COUNT", 1)?,
            queue_block_ms: get_u32(vars, "QUEUE_BLOCK_MS", 5000)?,
            queue_max_attempts: get_u32(vars, "QUEUE_MAX_ATTEMPTS", 3)?,
            queue_dlq_stream_name: get_string(vars, "queue_dlq_stream_name", "nsfw:queue:video_detection:dlq"),

            clickhouse_buffer_video_results_key: get_string(
                vars,
                "clickhouse_buffer_video_results_key",
                "nsfw:clickhouse_buffer:video_results",
            ),
            clickhouse_buffer_legacy_key: get_string(
                vars,
                "clickhouse_buffer_legacy_key",
                "nsfw:clickhouse_buffer:legacy_nsfw_agg",
            ),
            clickhouse_buffer_storage_actions_key: get_string(
                vars,
                "clickhouse_buffer_storage_actions_key",
                "nsfw:clickhouse_buffer:storage_actions",
            ),
            runtime_nsfw_key_prefix: get_string(vars, "runtime_nsfw_key_prefix", "offchain:video_nsfw:"),
        })
    }

    pub fn internal_request_secret(&self) -> Option<&SecretString> {
        self.internal_request_hmac_secret.as_ref()
    }

    pub fn is_kvrocks_configured(&self) -> bool {
        self.kvrocks_host.is_some()
    }

    pub fn is_gpu_configured(&self) -> bool {
        self.api_base_url.is_some() && self.api_key.is_some() && self.model_name.is_some()
    }

    pub fn is_clickhouse_configured(&self) -> bool {
        self.clickhouse_primary_database_url.is_some()
    }

    pub fn is_postgres_configured(&self) -> bool {
        self.postgres_database_url.is_some()
    }
}
```

**Note on secret redaction:** `secrecy::SecretString`'s own `Debug` impl already redacts its contents (prints a fixed placeholder, never the real value), and `#[derive(Debug)]` on `Settings` calls each field's own `Debug` impl — so no manual `Debug` implementation is needed here; the derive is sufficient as long as every secret field's type is `SecretString`/`Option<SecretString>`, not `String`.

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-config/src/lib.rs
pub mod settings;
pub use settings::*;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p nsfw-config --lib settings`
Expected: PASS (9 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-config/src/settings.rs crates/nsfw-config/src/lib.rs
git commit -m "feat: port static Settings struct with env aliases and secret redaction"
```

---

### Task 11: Phase 1 completion check

**Files:** none (verification only)

- [ ] **Step 1: Full workspace check**

Run: `cargo fmt --all -- --check`
Expected: no diff (run `cargo fmt --all` first if it fails, then re-check).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

Run: `cargo test --workspace`
Expected: all tests pass (`nsfw-core`: moderation, video_status, legacy_mapping, aggregation, models, error, model_output ≈ 63 tests; `nsfw-config`: settings ≈ 9 tests).

- [ ] **Step 2: Write a short completion note**

Append to this plan file's bottom (or a PR description, if opening one):
- Commands run: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Known gaps carried forward on purpose: `RuntimeConfig` (spec §8.2) is not implemented yet — it depends on the KVRocks repository, which doesn't exist until Phase 3/8. The 39-vs-45-column ClickHouse discrepancy (spec §13.2) and the KVRocks cluster-mode crate choice (spec §13.3) remain open, gated on Phase 3 per the spec.

- [ ] **Step 3: Final commit (if any formatting fixes were needed)**

```bash
git add -A
git commit -m "chore: phase 1 completion — fmt/clippy/test all green"
```

---

## What's Next

This plan covers Phase 1 only (spec §18). Phase 2 (API skeleton: axum app, HMAC middleware, `/health`/`/ready`, OpenAPI scaffold) gets its own plan document once this one is executed and reviewed — it depends on decisions (exact `AppError` → `IntoResponse` wiring, the `nsfw-services` crate's first inhabitant) that are easier to get right after `nsfw-core`/`nsfw-config` actually exist and compile.
