#![allow(dead_code)]

use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub enum SourceQuality {
    Live,
    FilteredDropout,
    Stale,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub timestamp: SystemTime,
    pub power_watts: u16,
    pub cadence_rpm: u16,
    pub crank_revolutions: u32,
    pub crank_event_time: u16,
    pub source_quality: SourceQuality,
}

impl Measurement {
    pub fn live(power_watts: u16, cadence_rpm: u16) -> Self {
        Self {
            timestamp: SystemTime::now(),
            power_watts,
            cadence_rpm,
            crank_revolutions: 0,
            crank_event_time: 0,
            source_quality: SourceQuality::Live,
        }
    }
}

#[derive(Debug)]
pub struct DropoutFilter {
    enabled: bool,
    previous_live: Option<(u16, u16)>,
    pending_zero: Option<Measurement>,
    zero_streak_confirmed: bool,
}

impl DropoutFilter {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            previous_live: None,
            pending_zero: None,
            zero_streak_confirmed: false,
        }
    }

    pub fn ingest(&mut self, mut measurement: Measurement) -> Vec<Measurement> {
        if !self.enabled {
            self.remember(&measurement);
            return vec![measurement];
        }

        let is_zero = measurement.power_watts == 0 && measurement.cadence_rpm == 0;
        if is_zero {
            if self.zero_streak_confirmed {
                return vec![measurement];
            }

            if let Some(previous_zero) = self.pending_zero.take() {
                self.zero_streak_confirmed = true;
                return vec![previous_zero, measurement];
            }

            self.pending_zero = Some(measurement);
            return Vec::new();
        }

        if self.pending_zero.take().is_some() {
            if self.previous_live.is_some() {
                measurement.source_quality = SourceQuality::FilteredDropout;
            }
        }
        self.zero_streak_confirmed = false;

        self.remember(&measurement);
        vec![measurement]
    }

    fn remember(&mut self, measurement: &Measurement) {
        if measurement.power_watts > 0 || measurement.cadence_rpm > 0 {
            self.previous_live = Some((measurement.power_watts, measurement.cadence_rpm));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppresses_single_zero_dropout_on_recovery_sample() {
        let mut filter = DropoutFilter::new(true);
        let first = filter.ingest(Measurement::live(120, 88));
        let zero = filter.ingest(Measurement::live(0, 0));
        let recovered = filter.ingest(Measurement::live(121, 89));

        assert_eq!(first[0].power_watts, 120);
        assert!(zero.is_empty());
        assert_eq!(recovered[0].power_watts, 121);
        assert_eq!(recovered[0].cadence_rpm, 89);
        assert_eq!(recovered[0].source_quality, SourceQuality::FilteredDropout);
    }
}
