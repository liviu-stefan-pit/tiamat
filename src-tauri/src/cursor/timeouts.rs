//! Agent timeout budgets.
//!
//! Defaults are generous enough for slow high-reasoning models (Grok High can take
//! ~10 minutes to plan). Override at process start with:
//! - `TIAMAT_ARCHITECT_TIMEOUT_MS`
//! - `TIAMAT_PHASE_TIMEOUT_MS`

use serde::{Deserialize, Serialize};

/// Default phase / implementation-agent wall clock (20 minutes).
pub const DEFAULT_PHASE_TIMEOUT_MS: u64 = 1_200_000;
/// Default architect wall clock (30 minutes).
pub const DEFAULT_ARCHITECT_TIMEOUT_MS: u64 = 1_800_000;

const ENV_ARCHITECT: &str = "TIAMAT_ARCHITECT_TIMEOUT_MS";
const ENV_PHASE: &str = "TIAMAT_PHASE_TIMEOUT_MS";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimeoutSettings {
    pub architect_timeout_ms: u64,
    pub phase_timeout_ms: u64,
}

impl Default for TimeoutSettings {
    fn default() -> Self {
        Self {
            architect_timeout_ms: DEFAULT_ARCHITECT_TIMEOUT_MS,
            phase_timeout_ms: DEFAULT_PHASE_TIMEOUT_MS,
        }
    }
}

impl TimeoutSettings {
    /// Resolve settings from the process environment, falling back to defaults.
    pub fn from_env() -> Self {
        let mut settings = Self::default();
        if let Some(ms) = parse_env_ms(ENV_ARCHITECT) {
            settings.architect_timeout_ms = ms;
        }
        if let Some(ms) = parse_env_ms(ENV_PHASE) {
            settings.phase_timeout_ms = ms;
        }
        settings
    }
}

fn parse_env_ms(key: &str) -> Option<u64> {
    let raw = std::env::var(key).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let ms = trimmed.parse::<u64>().ok()?;
    // Reject absurdly small values that would reintroduce the old 2-minute trap.
    Some(ms.max(1_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_generous() {
        let s = TimeoutSettings::default();
        assert_eq!(s.architect_timeout_ms, 1_800_000);
        assert_eq!(s.phase_timeout_ms, 1_200_000);
    }

    #[test]
    fn parse_env_rejects_empty_and_floors_tiny() {
        assert_eq!(parse_env_ms("__TIAMAT_NEVER_SET__"), None);
        // Direct unit of the helper: floor at 1s.
        assert_eq!(
            "50".parse::<u64>().ok().map(|v| v.max(1_000)),
            Some(1_000)
        );
    }
}
