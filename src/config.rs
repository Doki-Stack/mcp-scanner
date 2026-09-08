//! Runtime configuration loaded from the environment.

use doki_shared::error::Result;

/// All runtime configuration for the Scanner MCP.
// Most fields aren't read yet — only `port` is consumed so far. The rest
// are wired in as the services that need them (cloner, summarizer,
// storage, rate limiter) land in later tasks. Remove this allow once
// they're all consumed.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,

    pub database_url: String,

    pub minio_endpoint: String,
    pub minio_access_key: String,
    pub minio_secret_key: String,
    pub minio_bucket: String,

    pub dragonfly_url: String,
    pub rabbitmq_url: String,

    pub ollama_base_url: String,
    pub llm_model: String,

    /// Git clone timeout.
    pub clone_timeout_secs: u64,
    /// LLM summarization call timeout.
    pub llm_timeout_secs: u64,
    /// Minimum interval between scans of the same repo.
    pub scan_rate_limit_window_secs: u64,
    /// Max scans running concurrently for one org.
    pub max_concurrent_scans_per_org: u32,
    /// How long a scanned context is cached before a fresh scan is forced.
    pub cache_ttl_hours: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: env_parse_or("PORT", 3000)?,

            database_url: doki_shared::config::require_env("DATABASE_URL")?,

            minio_endpoint: doki_shared::config::require_env("MINIO_ENDPOINT")?,
            minio_access_key: doki_shared::config::require_env("MINIO_ACCESS_KEY")?,
            minio_secret_key: doki_shared::config::require_env("MINIO_SECRET_KEY")?,
            minio_bucket: doki_shared::config::env_or("MINIO_BUCKET", "scanner-artifacts"),

            dragonfly_url: doki_shared::config::require_env("DRAGONFLY_URL")?,
            rabbitmq_url: doki_shared::config::require_env("RABBITMQ_URL")?,

            ollama_base_url: doki_shared::config::require_env("OLLAMA_BASE_URL")?,
            llm_model: doki_shared::config::env_or("LLM_MODEL", "qwen2.5-coder"),

            clone_timeout_secs: env_parse_or("CLONE_TIMEOUT_SECS", 60)?,
            llm_timeout_secs: env_parse_or("LLM_TIMEOUT_SECS", 120)?,
            scan_rate_limit_window_secs: env_parse_or("SCAN_RATE_LIMIT_WINDOW_SECS", 300)?,
            max_concurrent_scans_per_org: env_parse_or("MAX_CONCURRENT_SCANS_PER_ORG", 10)?,
            cache_ttl_hours: env_parse_or("CACHE_TTL_HOURS", 24)?,
        })
    }
}

/// Parses an optional env var with a typed default, erroring only if the
/// variable is set but not parseable (missing => default, not an error).
fn env_parse_or<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    parse_or(std::env::var(key).ok().as_deref(), default)
        .map_err(|e| doki_shared::error::Error::internal(format!("invalid {key}: {e}")))
}

/// Pure parsing logic behind env_parse_or, split out so it's testable
/// without mutating process-global environment state.
fn parse_or<T>(raw: Option<&str>, default: T) -> std::result::Result<T, T::Err>
where
    T: std::str::FromStr,
{
    match raw {
        Some(val) => val.parse::<T>(),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_or_uses_default_when_absent() {
        let got: u16 = parse_or(None, 3000).unwrap();
        assert_eq!(got, 3000);
    }

    #[test]
    fn parse_or_parses_present_value() {
        let got: u16 = parse_or(Some("8080"), 3000).unwrap();
        assert_eq!(got, 8080);
    }

    #[test]
    fn parse_or_errors_on_unparseable_value() {
        let result: std::result::Result<u16, _> = parse_or(Some("not-a-number"), 3000);
        assert!(result.is_err());
    }

    #[test]
    fn from_env_loads_all_required_and_default_fields() {
        // SAFETY: test-only, single-threaded within this function's scope
        // via distinct env var names not touched by any other test in this
        // crate — see the module-level note on why this isn't factored
        // into per-field tests.
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://localhost/scanner");
            std::env::set_var("MINIO_ENDPOINT", "minio:9000");
            std::env::set_var("MINIO_ACCESS_KEY", "access");
            std::env::set_var("MINIO_SECRET_KEY", "secret");
            std::env::set_var("DRAGONFLY_URL", "dragonfly:6379");
            std::env::set_var("RABBITMQ_URL", "amqp://rabbitmq:5672");
            std::env::set_var("OLLAMA_BASE_URL", "http://ollama:11434");
        }

        let cfg = Config::from_env().expect("from_env should succeed with required vars set");

        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.database_url, "postgres://localhost/scanner");
        assert_eq!(cfg.minio_bucket, "scanner-artifacts");
        assert_eq!(cfg.llm_model, "qwen2.5-coder");
        assert_eq!(cfg.clone_timeout_secs, 60);
        assert_eq!(cfg.llm_timeout_secs, 120);
        assert_eq!(cfg.scan_rate_limit_window_secs, 300);
        assert_eq!(cfg.max_concurrent_scans_per_org, 10);
        assert_eq!(cfg.cache_ttl_hours, 24);

        unsafe {
            for key in [
                "DATABASE_URL",
                "MINIO_ENDPOINT",
                "MINIO_ACCESS_KEY",
                "MINIO_SECRET_KEY",
                "DRAGONFLY_URL",
                "RABBITMQ_URL",
                "OLLAMA_BASE_URL",
            ] {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn from_env_fails_when_required_var_missing() {
        // Deliberately does not set DATABASE_URL etc.
        unsafe {
            std::env::remove_var("DATABASE_URL");
        }
        assert!(Config::from_env().is_err());
    }
}
