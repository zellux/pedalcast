use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;

use crate::adapter::AdapterId;
use crate::error::PedalcastError;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub bike: BikeConfig,
    pub server: ServerConfig,
    pub timeouts: TimeoutConfig,
    pub filter: FilterConfig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BikeConfig {
    pub bike_type: String,
    pub adapter: AdapterId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    pub server_type: String,
    pub adapter: AdapterId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeoutConfig {
    pub telemetry_stale_ms: u64,
    pub bike_disconnect_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilterConfig {
    pub suppress_single_zero_dropouts: bool,
}

impl Config {
    pub fn from_path(path: &str) -> Result<Self, PedalcastError> {
        let content = fs::read_to_string(path).map_err(|source| PedalcastError::Io {
            path: path.to_string(),
            source,
        })?;
        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Result<Self, PedalcastError> {
        let values = parse_toml_subset(content)?;

        let bike_type = required_string(&values, "bike.type")?;
        if bike_type != "keiser_m3i" {
            return Err(PedalcastError::config(format!(
                "unsupported bike.type `{bike_type}`; expected `keiser_m3i`"
            )));
        }

        let server_type = required_string(&values, "server.type")?;
        if server_type != "ble" {
            return Err(PedalcastError::config(format!(
                "unsupported server.type `{server_type}`; expected `ble`"
            )));
        }

        Ok(Self {
            bike: BikeConfig {
                bike_type,
                adapter: required_adapter(&values, "bike.adapter")?,
            },
            server: ServerConfig {
                server_type,
                adapter: required_adapter(&values, "server.adapter")?,
                name: optional_string(&values, "server.name", "Pedalcast"),
            },
            timeouts: TimeoutConfig {
                telemetry_stale_ms: optional_u64(&values, "timeouts.telemetry_stale_ms", 3000)?,
                bike_disconnect_ms: optional_u64(&values, "timeouts.bike_disconnect_ms", 300000)?,
            },
            filter: FilterConfig {
                suppress_single_zero_dropouts: optional_bool(
                    &values,
                    "filter.suppress_single_zero_dropouts",
                    true,
                )?,
            },
        })
    }
}

fn parse_toml_subset(content: &str) -> Result<BTreeMap<String, String>, PedalcastError> {
    let mut section = String::new();
    let mut values = BTreeMap::new();

    for (line_number, raw_line) in content.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(PedalcastError::config(format!(
                "invalid config line {}: expected key = value",
                line_number + 1
            )));
        };

        let key = key.trim();
        let full_key = if section.is_empty() {
            key.to_string()
        } else {
            format!("{section}.{key}")
        };
        values.insert(full_key, value.trim().to_string());
    }

    Ok(values)
}

fn required_string(values: &BTreeMap<String, String>, key: &str) -> Result<String, PedalcastError> {
    values
        .get(key)
        .map(|value| trim_string(value))
        .ok_or_else(|| PedalcastError::config(format!("missing required config key `{key}`")))
}

fn optional_string(values: &BTreeMap<String, String>, key: &str, default: &str) -> String {
    values
        .get(key)
        .map(|value| trim_string(value))
        .unwrap_or_else(|| default.to_string())
}

fn required_adapter(
    values: &BTreeMap<String, String>,
    key: &str,
) -> Result<AdapterId, PedalcastError> {
    AdapterId::from_str(
        values.get(key).ok_or_else(|| {
            PedalcastError::config(format!("missing required config key `{key}`"))
        })?,
    )
}

fn optional_u64(
    values: &BTreeMap<String, String>,
    key: &str,
    default: u64,
) -> Result<u64, PedalcastError> {
    match values.get(key) {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| PedalcastError::config(format!("invalid integer for `{key}`"))),
        None => Ok(default),
    }
}

fn optional_bool(
    values: &BTreeMap<String, String>,
    key: &str,
    default: bool,
) -> Result<bool, PedalcastError> {
    match values.get(key).map(|value| value.as_str()) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(_) => Err(PedalcastError::config(format!(
            "invalid boolean for `{key}`"
        ))),
        None => Ok(default),
    }
}

fn trim_string(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config() {
        let config = Config::parse(
            r#"
            [bike]
            type = "keiser_m3i"
            adapter = "hci1"

            [server]
            type = "ble"
            adapter = 0
            name = "Pedalcast Test"
            "#,
        )
        .unwrap();

        assert_eq!(config.bike.adapter.index(), 1);
        assert_eq!(config.server.adapter.index(), 0);
        assert_eq!(config.server.name, "Pedalcast Test");
        assert!(config.filter.suppress_single_zero_dropouts);
    }
}
