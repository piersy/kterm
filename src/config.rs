use serde::Deserialize;

/// Default label key used to group "related components". This is the
/// Kubernetes recommended label for the instance an object belongs to.
pub const DEFAULT_RELATED_LABEL: &str = "app.kubernetes.io/instance";

/// User configuration, loaded once at startup from a YAML file.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    /// Label key whose value identifies "related components" — every object
    /// carrying the same value for this label is shown together.
    pub related_label: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            related_label: DEFAULT_RELATED_LABEL.to_string(),
        }
    }
}

impl Config {
    /// Load configuration from the first config path that exists, falling back
    /// to defaults when the file is absent or cannot be parsed. Loading never
    /// fails: a bad config degrades to defaults rather than aborting the app.
    pub fn load() -> Self {
        match Self::config_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(contents) => Self::parse(&contents),
            None => Self::default(),
        }
    }

    /// Parse YAML configuration text. On parse error, logs and returns
    /// defaults. Unknown/missing fields fall back to their defaults via
    /// `#[serde(default)]`.
    pub fn parse(contents: &str) -> Self {
        match serde_yaml::from_str(contents) {
            Ok(config) => config,
            Err(e) => {
                crate::logging::log_error(&format!(
                    "Failed to parse config, using defaults: {}",
                    e
                ));
                Self::default()
            }
        }
    }

    /// Resolve the config file path: `$XDG_CONFIG_HOME/kterm/config.yaml`,
    /// falling back to `$HOME/.config/kterm/config.yaml`.
    fn config_path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
            })?;
        Some(base.join("kterm").join("config.yaml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_related_label() {
        assert_eq!(Config::default().related_label, DEFAULT_RELATED_LABEL);
    }

    #[test]
    fn test_parse_overrides_label() {
        let cfg = Config::parse("related_label: app.kubernetes.io/part-of\n");
        assert_eq!(cfg.related_label, "app.kubernetes.io/part-of");
    }

    #[test]
    fn test_parse_empty_yields_defaults() {
        // An empty document and an unrelated key both fall back to defaults.
        assert_eq!(Config::parse("{}"), Config::default());
        assert_eq!(Config::parse("other: 1\n"), Config::default());
    }

    #[test]
    fn test_parse_invalid_yields_defaults() {
        // Malformed YAML must not panic; it degrades to defaults.
        assert_eq!(Config::parse(": : not yaml : :"), Config::default());
    }
}
