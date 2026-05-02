#![allow(dead_code)]

use crate::error::PedalcastError;
use crate::telemetry::Measurement;

const KEISER_VALUE_MAGIC: [u8; 2] = [0x02, 0x01];
const IDX_VER_MAJOR: usize = 2;
const IDX_VER_MINOR: usize = 3;
const IDX_REALTIME: usize = 4;
const IDX_CADENCE: usize = 6;
const IDX_POWER: usize = 10;
const MIN_PAYLOAD_LEN: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BikeVersion {
    pub major: u8,
    pub minor: u8,
    pub stats_timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeiserStats {
    pub power_watts: u16,
    pub cadence_rpm: u16,
}

impl KeiserStats {
    pub fn into_measurement(self) -> Measurement {
        Measurement::live(self.power_watts, self.cadence_rpm)
    }
}

pub fn bike_version(payload: &[u8]) -> Result<BikeVersion, PedalcastError> {
    require_keiser_payload(payload)?;

    let major = payload[IDX_VER_MAJOR];
    let minor = payload[IDX_VER_MINOR];
    let stats_timeout_ms = if major == 0x06 && minor >= 0x30 {
        1000
    } else {
        7000
    };

    Ok(BikeVersion {
        major,
        minor,
        stats_timeout_ms,
    })
}

pub fn parse_stats(payload: &[u8]) -> Result<KeiserStats, PedalcastError> {
    require_keiser_payload(payload)?;

    let realtime = payload[IDX_REALTIME];
    if realtime != 0 && !(realtime > 128 && realtime < 255) {
        return Err(PedalcastError::runtime(
            "unable to parse Keiser message: payload is not realtime data",
        ));
    }

    Ok(KeiserStats {
        power_watts: read_u16_le(payload, IDX_POWER),
        cadence_rpm: (read_u16_le(payload, IDX_CADENCE) + 5) / 10,
    })
}

fn require_keiser_payload(payload: &[u8]) -> Result<(), PedalcastError> {
    if payload.len() < MIN_PAYLOAD_LEN {
        return Err(PedalcastError::runtime(format!(
            "unable to parse Keiser message: payload too short len={}",
            payload.len()
        )));
    }

    if payload[0..2] != KEISER_VALUE_MAGIC[..] {
        return Err(PedalcastError::runtime(
            "unable to parse Keiser message: missing magic 0201",
        ));
    }

    Ok(())
}

fn read_u16_le(payload: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([payload[offset], payload[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_realtime_stats() {
        let payload = [
            0x02, 0x01, 0x06, 0x30, 0x00, 0x00, 0x70, 0x03, 0x00, 0x00, 0xfa, 0x00,
        ];

        let stats = parse_stats(&payload).unwrap();

        assert_eq!(stats.power_watts, 250);
        assert_eq!(stats.cadence_rpm, 88);
        assert_eq!(bike_version(&payload).unwrap().stats_timeout_ms, 1000);
    }
}
