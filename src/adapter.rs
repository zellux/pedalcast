use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use crate::error::PedalcastError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AdapterId(u8);

impl AdapterId {
    #[allow(dead_code)]
    pub fn index(self) -> u8 {
        self.0
    }
}

pub trait DisplayAdapter {
    fn to_hci_string(&self) -> String;
}

impl DisplayAdapter for u8 {
    fn to_hci_string(&self) -> String {
        format!("hci{self}")
    }
}

impl DisplayAdapter for AdapterId {
    fn to_hci_string(&self) -> String {
        self.0.to_hci_string()
    }
}

impl std::fmt::Display for AdapterId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_hci_string())
    }
}

impl FromStr for AdapterId {
    type Err = PedalcastError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim().trim_matches('"');
        let digits = trimmed.strip_prefix("hci").unwrap_or(trimmed);
        let index = digits.parse::<u8>().map_err(|_| {
            PedalcastError::config(format!(
                "invalid adapter `{value}`; expected number or hciN"
            ))
        })?;
        Ok(Self(index))
    }
}

#[derive(Debug)]
pub struct AdapterRegistry {
    available: BTreeSet<AdapterId>,
}

impl AdapterRegistry {
    pub fn detect() -> Result<Self, PedalcastError> {
        if let Ok(mocked) = env::var("PEDALCAST_ADAPTERS") {
            return Self::from_list(mocked.split(','));
        }

        let sysfs = Path::new("/sys/class/bluetooth");
        let entries = fs::read_dir(sysfs).map_err(|source| PedalcastError::Io {
            path: sysfs.display().to_string(),
            source,
        })?;

        let mut available = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|source| PedalcastError::Io {
                path: sysfs.display().to_string(),
                source,
            })?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(adapter) = AdapterId::from_str(&name) {
                available.insert(adapter);
            }
        }

        Ok(Self { available })
    }

    fn from_list<'a, I>(items: I) -> Result<Self, PedalcastError>
    where
        I: Iterator<Item = &'a str>,
    {
        let mut available = BTreeSet::new();
        for item in items {
            let item = item.trim();
            if !item.is_empty() {
                available.insert(AdapterId::from_str(item)?);
            }
        }
        Ok(Self { available })
    }

    pub fn require(&self, role: &str, adapter: AdapterId) -> Result<(), PedalcastError> {
        if self.available.contains(&adapter) {
            return Ok(());
        }

        Err(PedalcastError::adapter(format!(
            "{role} adapter {adapter} not found; available adapters: {}",
            self.available_adapters()
        )))
    }

    pub fn available_adapters(&self) -> String {
        if self.available.is_empty() {
            return "none".to_string();
        }

        self.available
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_and_hci_adapter_ids() {
        assert_eq!(AdapterId::from_str("1").unwrap().index(), 1);
        assert_eq!(AdapterId::from_str("hci7").unwrap().index(), 7);
    }
}
