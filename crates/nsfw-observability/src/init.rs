//! Installs the process-wide Sentry client + tracing subscriber stack. Call ONCE from
//! `main`, before the async runtime starts, and hold the returned guard for the whole
//! process so buffered Sentry events flush on shutdown.

use nsfw_config::Settings;
use secrecy::ExposeSecret;
use sentry::ClientInitGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Held for process lifetime. When `sentry_dsn` is unset the inner guard is `None`
/// (logging still works, Sentry is inert) — matching Python's no-DSN behavior.
#[must_use = "hold the guard for the process lifetime so Sentry events flush on exit"]
pub struct ObservabilityGuard {
    _sentry: Option<ClientInitGuard>,
}

pub fn init(settings: &Settings) -> ObservabilityGuard {
    // 1) Sentry — DSN-gated. Installs the default integrations, including the panic hook.
    let sentry_guard = settings.sentry_dsn.as_ref().map(|dsn| {
        sentry::init((
            dsn.expose_secret().to_string(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: Some(settings.environment.clone().into()),
                send_default_pii: settings.sentry_send_default_pii,
                ..Default::default()
            },
        ))
    });

    // 2) Subscriber stack. EnvFilter (RUST_LOG, default info) + JSON fmt (-> journald) +
    //    sentry-tracing (default EventFilter: ERROR->event, WARN/INFO->breadcrumb,
    //    DEBUG/TRACE->ignored — this is the content->Sentry barrier, see spec §2/§4).
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().json())
        .with(sentry_tracing::layer())
        .init();

    ObservabilityGuard {
        _sentry: sentry_guard,
    }
}
