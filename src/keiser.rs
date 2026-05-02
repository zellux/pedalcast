#![allow(dead_code)]

use crate::error::PedalcastError;
use crate::telemetry::Measurement;

const KEISER_VALUE_MAGIC: [u8; 2] = [0x02, 0x01];
const IDX_VER_MAJOR: usize = 2;
const IDX_VER_MINOR: usize = 3;
const IDX_REALTIME: usize = 4;
const IDX_ID: usize = 5;
const IDX_CADENCE: usize = 6;
const IDX_HEART_RATE: usize = 8;
const IDX_POWER: usize = 10;
const IDX_ENERGY: usize = 12;
const IDX_TIME_MINUTES: usize = 14;
const IDX_TIME_SECONDS: usize = 15;
const IDX_TRIP: usize = 16;
const IDX_GEAR: usize = 18;
const MIN_PAYLOAD_LEN: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BikeVersion {
    pub major: u8,
    pub minor: u8,
    pub stats_timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeiserStats {
    pub interval: u8,
    pub id: u8,
    pub power_watts: u16,
    pub cadence_rpm: u16,
    pub heart_rate_bpm: u16,
    pub energy_kcal: u16,
    pub elapsed_seconds: u16,
    pub trip: u32,
    pub gear: Option<u8>,
}

impl KeiserStats {
    pub fn is_realtime(&self) -> bool {
        self.interval == 0 || (self.interval > 128 && self.interval < 255)
    }

    pub fn into_measurement(self) -> Measurement {
        Measurement::live(self.power_watts, self.cadence_rpm)
    }
}

pub fn bike_version(payload: &[u8]) -> Result<BikeVersion, PedalcastError> {
    require_keiser_payload(payload)?;

    let major = build_value(payload[IDX_VER_MAJOR])?;
    let minor = build_value(payload[IDX_VER_MINOR])?;
    let stats_timeout_ms = if major == 6 && minor >= 30 {
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

    let version = bike_version(payload)?;
    if version.major != 6 {
        return Err(PedalcastError::runtime(format!(
            "unable to parse Keiser message: unsupported build major {}",
            version.major
        )));
    }

    Ok(KeiserStats {
        interval: payload[IDX_REALTIME],
        id: payload[IDX_ID],
        power_watts: read_u16_le(payload, IDX_POWER),
        cadence_rpm: read_u16_le(payload, IDX_CADENCE) / 10,
        heart_rate_bpm: read_u16_le(payload, IDX_HEART_RATE) / 10,
        energy_kcal: read_u16_le_if_present(payload, IDX_ENERGY).unwrap_or_default(),
        elapsed_seconds: elapsed_seconds(payload).unwrap_or_default(),
        trip: parse_trip(payload).unwrap_or_default(),
        gear: if version.minor >= 21 && payload.len() > IDX_GEAR {
            Some(payload[IDX_GEAR])
        } else {
            None
        },
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

fn read_u16_le_if_present(payload: &[u8], offset: usize) -> Option<u16> {
    (payload.len() > offset + 1).then(|| read_u16_le(payload, offset))
}

fn elapsed_seconds(payload: &[u8]) -> Option<u16> {
    (payload.len() > IDX_TIME_SECONDS).then(|| {
        u16::from(payload[IDX_TIME_MINUTES]) * 60 + u16::from(payload[IDX_TIME_SECONDS])
    })
}

fn parse_trip(payload: &[u8]) -> Option<u32> {
    let trip = read_u16_le_if_present(payload, IDX_TRIP)?;
    if trip & 0x8000 != 0 {
        Some((f64::from(trip) * 1.60934) as u32)
    } else {
        Some(u32::from(trip))
    }
}

fn build_value(value: u8) -> Result<u8, PedalcastError> {
    let hex_digits = format!("{value:X}");
    hex_digits.parse::<u8>().map_err(|source| {
        PedalcastError::runtime(format!(
            "unable to parse Keiser message: invalid build value {value:#04x}: {source}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_realtime_stats() {
        let payload = [
            0x02, 0x01, 0x06, 0x30, 0x00, 0x00, 0x70, 0x03, 0x00, 0x00, 0xfa, 0x00, 0x0a, 0x00,
            0x01, 0x02, 0x34, 0x12, 0x08,
        ];

        let stats = parse_stats(&payload).unwrap();

        assert_eq!(stats.interval, 0);
        assert_eq!(stats.id, 0);
        assert_eq!(stats.power_watts, 250);
        assert_eq!(stats.cadence_rpm, 88);
        assert_eq!(stats.heart_rate_bpm, 0);
        assert_eq!(stats.energy_kcal, 10);
        assert_eq!(stats.elapsed_seconds, 62);
        assert_eq!(stats.trip, 0x1234);
        assert_eq!(stats.gear, Some(8));
        assert_eq!(bike_version(&payload).unwrap().stats_timeout_ms, 1000);
        assert_eq!(bike_version(&payload).unwrap().minor, 30);
    }

    #[test]
    fn truncates_decimal_cadence_like_keiser_parser() {
        let payload = [
            0x02, 0x01, 0x06, 0x30, 0x00, 0x00, 0xfb, 0x03, 0x00, 0x00, 0xfa, 0x00, 0x0a, 0x00,
            0x01, 0x02, 0x34, 0x12,
        ];

        let stats = parse_stats(&payload).unwrap();

        assert_eq!(stats.cadence_rpm, 101);
    }

    #[test]
    fn preserves_short_live_power_cadence_packets() {
        let payload = [
            0x02, 0x01, 0x06, 0x30, 0x00, 0x00, 0x70, 0x03, 0x00, 0x00, 0xfa, 0x00,
        ];

        let stats = parse_stats(&payload).unwrap();

        assert!(stats.is_realtime());
        assert_eq!(stats.power_watts, 250);
        assert_eq!(stats.cadence_rpm, 88);
        assert_eq!(stats.energy_kcal, 0);
        assert_eq!(stats.elapsed_seconds, 0);
        assert_eq!(stats.trip, 0);
    }

    #[test]
    fn rejects_unsupported_build_major() {
        let payload = [
            0x02, 0x01, 0x07, 0x30, 0x00, 0x00, 0x70, 0x03, 0x00, 0x00, 0xfa, 0x00, 0x0a, 0x00,
            0x01, 0x02, 0x34, 0x12,
        ];

        assert!(parse_stats(&payload).is_err());
    }

    #[test]
    fn converts_metric_trip_flag_like_keiser_parser() {
        let payload = [
            0x02, 0x01, 0x06, 0x30, 0x00, 0x00, 0x70, 0x03, 0x00, 0x00, 0xfa, 0x00, 0x0a, 0x00,
            0x01, 0x02, 0x00, 0x80,
        ];

        let stats = parse_stats(&payload).unwrap();

        assert_eq!(stats.trip, 52734);
    }

    #[test]
    fn parses_average_packets_but_marks_them_non_realtime() {
        let payload = [
            0x02, 0x01, 0x06, 0x30, 0x05, 0x00, 0x70, 0x03, 0x00, 0x00, 0xfa, 0x00, 0x0a, 0x00,
            0x01, 0x02, 0x34, 0x12,
        ];

        let stats = parse_stats(&payload).unwrap();

        assert!(!stats.is_realtime());
    }
}
