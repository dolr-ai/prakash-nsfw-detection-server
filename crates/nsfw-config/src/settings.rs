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
    vars.get(name)
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn get_secret(vars: &HashMap<String, String>, name: &str) -> Option<SecretString> {
    vars.get(name).cloned().map(SecretString::from)
}

fn get_bool(
    vars: &HashMap<String, String>,
    name: &str,
    default: bool,
) -> Result<bool, ConfigError> {
    match vars.get(name) {
        Some(v) => v
            .parse::<bool>()
            .map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_u16(vars: &HashMap<String, String>, name: &str, default: u16) -> Result<u16, ConfigError> {
    match vars.get(name) {
        Some(v) => v
            .parse::<u16>()
            .map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_u32(vars: &HashMap<String, String>, name: &str, default: u32) -> Result<u32, ConfigError> {
    match vars.get(name) {
        Some(v) => v
            .parse::<u32>()
            .map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_i64(vars: &HashMap<String, String>, name: &str, default: i64) -> Result<i64, ConfigError> {
    match vars.get(name) {
        Some(v) => v
            .parse::<i64>()
            .map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_f64(vars: &HashMap<String, String>, name: &str, default: f64) -> Result<f64, ConfigError> {
    match vars.get(name) {
        Some(v) => v
            .parse::<f64>()
            .map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
        None => Ok(default),
    }
}

fn get_u64(vars: &HashMap<String, String>, name: &str, default: u64) -> Result<u64, ConfigError> {
    match vars.get(name) {
        Some(v) => v
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidValue(name.to_string(), v.clone())),
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

    /// Rust-only addition, not present in Python (spec §6.4) -- sqlx pool sizing,
    /// tuned independently per binary via the same env var, different deployed values.
    pub postgres_pool_max_connections: u32,
    pub postgres_pool_min_connections: u32,
    pub postgres_pool_acquire_timeout_seconds: f64,
    /// Rust-only addition, not present in Python (spec §8.2) -- how often api/video-worker/
    /// flush-worker poll KVRocks for RuntimeConfig changes. RuntimeConfig itself isn't
    /// implemented until Phase 3/8 (needs the KVRocks repository); this field just reserves
    /// the static setting that controls its poll cadence once it exists.
    pub runtime_config_poll_interval_seconds: u32,
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_map(&std::env::vars().collect())
    }

    /// pydantic-settings' `BaseSettings` matches env vars case-insensitively by default
    /// (no `case_sensitive=True` anywhere in the Python source) -- normalize the incoming
    /// map to uppercase keys once, so this Rust port accepts either casing convention
    /// exactly like Python does, instead of silently falling back to defaults if a real
    /// deployment's `.env` happens to use a different case than this file's literals.
    pub fn from_map(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_uppercase(), v.clone()))
            .collect();
        let vars = &vars;
        Ok(Self {
            app_name: get_string(vars, "APP_NAME", "yral-nsfw-detector"),
            environment: get_string(vars, "ENVIRONMENT", "local"),

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
            kvrocks_pool_retry_base_delay_seconds: get_f64(
                vars,
                "KVROCKS_POOL_RETRY_BASE_DELAY_SECONDS",
                0.05,
            )?,
            kvrocks_socket_timeout_seconds: get_f64(vars, "KVROCKS_SOCKET_TIMEOUT_SECONDS", 5.0)?,
            kvrocks_socket_connect_timeout_seconds: get_f64(
                vars,
                "KVROCKS_SOCKET_CONNECT_TIMEOUT_SECONDS",
                5.0,
            )?,
            kvrocks_health_check_interval_seconds: get_u32(
                vars,
                "KVROCKS_HEALTH_CHECK_INTERVAL_SECONDS",
                30,
            )?,
            kvrocks_ssl_ca_cert: vars.get("KVROCKS_SSL_CA_CERT").cloned(),
            kvrocks_ssl_client_cert: vars.get("KVROCKS_SSL_CLIENT_CERT").cloned(),
            kvrocks_ssl_client_key: vars.get("KVROCKS_SSL_CLIENT_KEY").cloned(),

            clickhouse_primary_database_url: get_secret(vars, "CLICKHOUSE_PRIMARY_DATABASE_URL"),
            clickhouse_secondary_database_url: get_secret(
                vars,
                "CLICKHOUSE_SECONDARY_DATABASE_URL",
            ),
            clickhouse_secure: get_bool(vars, "CLICKHOUSE_SECURE", true)?,
            clickhouse_verify: get_bool(vars, "CLICKHOUSE_VERIFY", true)?,
            clickhouse_database: get_string(vars, "CLICKHOUSE_DATABASE", "yral"),
            clickhouse_user: get_secret(vars, "CLICKHOUSE_USER"),
            clickhouse_password: get_secret(vars, "CLICKHOUSE_PASSWORD"),
            clickhouse_nsfw_table: get_string(
                vars,
                "CLICKHOUSE_NSFW_TABLE",
                "video_nsfw_detection",
            ),
            clickhouse_nsfw_agg_table: get_string(
                vars,
                "CLICKHOUSE_NSFW_AGG_TABLE",
                "video_nsfw_agg",
            ),
            clickhouse_excluded_videos_table: get_string(
                vars,
                "CLICKHOUSE_EXCLUDED_VIDEOS_TABLE",
                "excluded_videos",
            ),
            clickhouse_storage_actions_table: get_string(
                vars,
                "CLICKHOUSE_STORAGE_ACTIONS_TABLE",
                "video_nsfw_storage_actions",
            ),

            storj_interface_url: vars.get("STORJ_INTERFACE_URL").cloned(),
            storj_interface_token: get_secret(vars, "STORJ_INTERFACE_TOKEN"),
            storj_interface_timeout_seconds: get_f64(
                vars,
                "STORJ_INTERFACE_TIMEOUT_SECONDS",
                10.0,
            )?,

            api_base_url: get_first(vars, &["API_BASE_URL", "API_BASE_URL "]).map(str::to_string),
            api_key: get_first(vars, &["API_KEY", "API_KEY "])
                .map(|v| SecretString::from(v.to_string())),
            model_name: get_first(vars, &["MODEL_NAME", "MODEL_NAME "]).map(str::to_string),
            model_provider: get_string(vars, "MODEL_PROVIDER", "openai-compatible"),
            model_version: vars.get("MODEL_VERSION").cloned(),

            sentry_dsn: get_secret(vars, "SENTRY_DSN"),
            sentry_send_default_pii: get_bool(vars, "SENTRY_SEND_DEFAULT_PII", false)?,

            default_policy_version: get_string(vars, "DEFAULT_POLICY_VERSION", "nsfw_policy_v1"),
            visual_prompt_version: get_string(
                vars,
                "VISUAL_PROMPT_VERSION",
                "visual_batch_moderation_v1",
            ),
            image_prompt_version: get_string(
                vars,
                "IMAGE_PROMPT_VERSION",
                "image_generation_moderation_v1",
            ),
            image_text_prompt_version: get_string(
                vars,
                "IMAGE_TEXT_PROMPT_VERSION",
                "image_prompt_generation_moderation_v1",
            ),
            text_prompt_version: get_string(vars, "TEXT_PROMPT_VERSION", "text_moderation_v1"),
            aggregation_version: get_string(vars, "AGGREGATION_VERSION", "hard_any_frame_v1"),

            frame_batch_size: get_u32(vars, "FRAME_BATCH_SIZE", 5)?,
            gpu_max_concurrency: get_u32(vars, "GPU_MAX_CONCURRENCY", 5)?,
            gpu_max_attempts: get_u32(vars, "GPU_MAX_ATTEMPTS", 3)?,
            gpu_retry_base_delay_seconds: get_f64(vars, "GPU_RETRY_BASE_DELAY_SECONDS", 0.25)?,

            image_max_bytes: get_u64(vars, "IMAGE_MAX_BYTES", 10 * 1024 * 1024)?,
            image_download_timeout_seconds: get_f64(vars, "IMAGE_DOWNLOAD_TIMEOUT_SECONDS", 30.0)?,
            image_download_max_attempts: get_u32(vars, "IMAGE_DOWNLOAD_MAX_ATTEMPTS", 3)?,
            image_download_retry_base_delay_seconds: get_f64(
                vars,
                "IMAGE_DOWNLOAD_RETRY_BASE_DELAY_SECONDS",
                0.5,
            )?,

            video_download_timeout_seconds: get_f64(vars, "VIDEO_DOWNLOAD_TIMEOUT_SECONDS", 120.0)?,
            video_max_bytes: get_u64(vars, "VIDEO_MAX_BYTES", 512 * 1024 * 1024)?,
            video_temp_root: get_string(vars, "VIDEO_TEMP_ROOT", "/tmp/nsfw"),
            ffprobe_timeout_seconds: get_f64(vars, "FFPROBE_TIMEOUT_SECONDS", 30.0)?,
            ffmpeg_timeout_seconds: get_f64(vars, "FFMPEG_TIMEOUT_SECONDS", 300.0)?,

            move_threshold: get_f64(vars, "MOVE_THRESHOLD", 0.8)?,

            queue_stream_name: get_string(vars, "QUEUE_STREAM_NAME", "nsfw:queue:video_detection"),
            queue_group_name: get_string(vars, "QUEUE_GROUP_NAME", "nsfw_video_workers"),
            queue_consumer_name: vars.get("QUEUE_CONSUMER_NAME").cloned(),
            queue_read_count: get_u32(vars, "QUEUE_READ_COUNT", 1)?,
            queue_block_ms: get_u32(vars, "QUEUE_BLOCK_MS", 5000)?,
            queue_max_attempts: get_u32(vars, "QUEUE_MAX_ATTEMPTS", 3)?,
            queue_dlq_stream_name: get_string(
                vars,
                "QUEUE_DLQ_STREAM_NAME",
                "nsfw:queue:video_detection:dlq",
            ),

            clickhouse_buffer_video_results_key: get_string(
                vars,
                "CLICKHOUSE_BUFFER_VIDEO_RESULTS_KEY",
                "nsfw:clickhouse_buffer:video_results",
            ),
            clickhouse_buffer_legacy_key: get_string(
                vars,
                "CLICKHOUSE_BUFFER_LEGACY_KEY",
                "nsfw:clickhouse_buffer:legacy_nsfw_agg",
            ),
            clickhouse_buffer_storage_actions_key: get_string(
                vars,
                "CLICKHOUSE_BUFFER_STORAGE_ACTIONS_KEY",
                "nsfw:clickhouse_buffer:storage_actions",
            ),
            runtime_nsfw_key_prefix: get_string(
                vars,
                "RUNTIME_NSFW_KEY_PREFIX",
                "offchain:video_nsfw:",
            ),

            postgres_pool_max_connections: get_u32(vars, "POSTGRES_POOL_MAX_CONNECTIONS", 10)?,
            postgres_pool_min_connections: get_u32(vars, "POSTGRES_POOL_MIN_CONNECTIONS", 1)?,
            postgres_pool_acquire_timeout_seconds: get_f64(
                vars,
                "POSTGRES_POOL_ACQUIRE_TIMEOUT_SECONDS",
                30.0,
            )?,
            runtime_config_poll_interval_seconds: get_u32(
                vars,
                "RUNTIME_CONFIG_POLL_INTERVAL_SECONDS",
                15,
            )?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
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
        let settings =
            Settings::from_map(&map(&[("API_BASE_URL ", "https://gpu.example.com")])).unwrap();
        assert_eq!(
            settings.api_base_url.as_deref(),
            Some("https://gpu.example.com")
        );
    }

    #[test]
    fn api_base_url_without_trailing_space_still_works() {
        let settings =
            Settings::from_map(&map(&[("API_BASE_URL", "https://gpu.example.com")])).unwrap();
        assert_eq!(
            settings.api_base_url.as_deref(),
            Some("https://gpu.example.com")
        );
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

        let partial =
            Settings::from_map(&map(&[("API_BASE_URL", "https://x"), ("API_KEY", "k")])).unwrap();
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
    fn is_clickhouse_configured_requires_primary_database_url() {
        let none = Settings::from_map(&HashMap::new()).unwrap();
        assert!(!none.is_clickhouse_configured());
        let configured = Settings::from_map(&map(&[(
            "CLICKHOUSE_PRIMARY_DATABASE_URL",
            "https://ch.example.com",
        )]))
        .unwrap();
        assert!(configured.is_clickhouse_configured());
    }

    #[test]
    fn is_postgres_configured_requires_database_url() {
        let none = Settings::from_map(&HashMap::new()).unwrap();
        assert!(!none.is_postgres_configured());
        let configured = Settings::from_map(&map(&[(
            "POSTGRES_DATABASE_URL",
            "postgresql://localhost/nsfw",
        )]))
        .unwrap();
        assert!(configured.is_postgres_configured());
    }

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let settings = Settings::from_map(&map(&[(
            "INTERNAL_REQUEST_HMAC_SECRET",
            "super-secret-value",
        )]))
        .unwrap();
        let debug_output = format!("{settings:?}");
        assert!(!debug_output.contains("super-secret-value"));
    }

    #[test]
    fn lowercase_field_name_settings_are_matched_case_insensitively() {
        // pydantic-settings matches env vars case-insensitively -- a real deployment might
        // set MOVE_THRESHOLD (shell convention) even though the Python attribute is
        // lowercase `move_threshold`. This must keep working in the Rust port too.
        let settings = Settings::from_map(&map(&[("MOVE_THRESHOLD", "0.5")])).unwrap();
        assert_eq!(settings.move_threshold, 0.5);

        let settings = Settings::from_map(&map(&[("queue_stream_name", "custom:stream")])).unwrap();
        assert_eq!(settings.queue_stream_name, "custom:stream");
    }

    #[test]
    fn postgres_pool_and_runtime_config_poll_settings_have_sane_defaults() {
        // Rust-only additions (spec §6.4/§8.2) -- not present in Python, added because
        // this port needs explicit pool sizing and a poll cadence Python never had.
        let settings = Settings::from_map(&HashMap::new()).unwrap();
        assert_eq!(settings.postgres_pool_max_connections, 10);
        assert_eq!(settings.postgres_pool_min_connections, 1);
        assert_eq!(settings.postgres_pool_acquire_timeout_seconds, 30.0);
        assert_eq!(settings.runtime_config_poll_interval_seconds, 15);
    }

    #[test]
    fn postgres_pool_and_runtime_config_poll_settings_are_configurable() {
        let settings = Settings::from_map(&map(&[
            ("POSTGRES_POOL_MAX_CONNECTIONS", "50"),
            ("RUNTIME_CONFIG_POLL_INTERVAL_SECONDS", "30"),
        ]))
        .unwrap();
        assert_eq!(settings.postgres_pool_max_connections, 50);
        assert_eq!(settings.runtime_config_poll_interval_seconds, 30);
    }
}
