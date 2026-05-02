use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use zbus::block_on;
use zbus::blocking::{Connection, Proxy};
use zbus::fdo::ObjectManager;
use zbus::interface;
use zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

use crate::adapter::AdapterId;
use crate::error::PedalcastError;
use crate::log;
use crate::telemetry::Measurement;

const APP_ROOT: &str = "/com/pedalcast";
const CPS_SERVICE_PATH: &str = "/com/pedalcast/service0";
const MEASUREMENT_PATH: &str = "/com/pedalcast/service0/char0";
const FEATURE_PATH: &str = "/com/pedalcast/service0/char1";
const SENSOR_LOCATION_PATH: &str = "/com/pedalcast/service0/char2";

const CPS_UUID: &str = "00001818-0000-1000-8000-00805f9b34fb";
const MEASUREMENT_UUID: &str = "00002a63-0000-1000-8000-00805f9b34fb";
const FEATURE_UUID: &str = "00002a65-0000-1000-8000-00805f9b34fb";
const SENSOR_LOCATION_UUID: &str = "00002a5d-0000-1000-8000-00805f9b34fb";

pub struct CyclingPowerGatt {
    adapter: AdapterId,
    telemetry_rx: Receiver<Measurement>,
}

impl CyclingPowerGatt {
    pub fn new(adapter: AdapterId, telemetry_rx: Receiver<Measurement>) -> Self {
        Self {
            adapter,
            telemetry_rx,
        }
    }

    pub fn start(self) {
        log::info(
            "app.gatt",
            "starting",
            &[("adapter", self.adapter.to_string())],
        );
        thread::spawn(move || {
            if let Err(error) = self.run() {
                log::error("app.gatt", "failed", &[("error", error.to_string())]);
            }
        });
    }

    fn run(self) -> Result<(), PedalcastError> {
        log::info(
            "app.gatt",
            "connecting_dbus",
            &[("adapter", self.adapter.to_string())],
        );
        let connection = Connection::system()
            .map_err(|source| PedalcastError::runtime(format!("D-Bus connect failed: {source}")))?;

        connection
            .object_server()
            .at(APP_ROOT, ObjectManager)
            .map_err(|source| {
                PedalcastError::runtime(format!("ObjectManager export failed: {source}"))
            })?;
        connection
            .object_server()
            .at(CPS_SERVICE_PATH, CyclingPowerService)
            .map_err(|source| {
                PedalcastError::runtime(format!("GATT service export failed: {source}"))
            })?;
        connection
            .object_server()
            .at(MEASUREMENT_PATH, MeasurementCharacteristic::default())
            .map_err(|source| {
                PedalcastError::runtime(format!("measurement char export failed: {source}"))
            })?;
        connection
            .object_server()
            .at(FEATURE_PATH, FeatureCharacteristic)
            .map_err(|source| {
                PedalcastError::runtime(format!("feature char export failed: {source}"))
            })?;
        connection
            .object_server()
            .at(SENSOR_LOCATION_PATH, SensorLocationCharacteristic)
            .map_err(|source| {
                PedalcastError::runtime(format!("sensor location export failed: {source}"))
            })?;

        let adapter_path = format!("/org/bluez/{}", self.adapter);
        let manager = Proxy::new(
            &connection,
            "org.bluez",
            adapter_path.as_str(),
            "org.bluez.GattManager1",
        )
        .map_err(|source| PedalcastError::runtime(format!("GattManager proxy failed: {source}")))?;

        let options: HashMap<&str, OwnedValue> = HashMap::new();
        let app_path = ObjectPath::try_from(APP_ROOT)
            .map_err(|source| PedalcastError::runtime(format!("invalid app path: {source}")))?;
        manager
            .call::<_, _, ()>("RegisterApplication", &(app_path, options))
            .map_err(|source| {
                PedalcastError::runtime(format!("RegisterApplication failed: {source}"))
            })?;

        log::info(
            "app.gatt",
            "registered",
            &[
                ("adapter", self.adapter.to_string()),
                ("service", "cycling_power".to_string()),
            ],
        );

        let measurement = connection
            .object_server()
            .interface::<_, MeasurementCharacteristic>(MEASUREMENT_PATH)
            .map_err(|source| {
                PedalcastError::runtime(format!("measurement interface lookup failed: {source}"))
            })?;
        let mut cadence = CadenceState::default();

        loop {
            match self.telemetry_rx.recv_timeout(Duration::from_secs(60)) {
                Ok(sample) => {
                    let mut iface = measurement.get_mut();
                    iface.apply_measurement(&sample, &mut cadence);
                    let notifying = iface.notifying;
                    if notifying {
                        block_on(iface.value_changed(measurement.signal_emitter())).map_err(
                            |source| {
                                PedalcastError::runtime(format!(
                                    "measurement notify failed: {source}"
                                ))
                            },
                        )?;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    log::info(
                        "app.gatt",
                        "heartbeat",
                        &[("service", "cycling_power".to_string())],
                    );
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(PedalcastError::runtime("telemetry channel disconnected"));
                }
            }
        }
    }
}

struct CyclingPowerService;

#[interface(name = "org.bluez.GattService1")]
impl CyclingPowerService {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        CPS_UUID
    }

    #[zbus(property)]
    fn primary(&self) -> bool {
        true
    }
}

#[derive(Default)]
struct MeasurementCharacteristic {
    power_watts: i16,
    crank_revolutions: u16,
    crank_event_time: u16,
    notifying: bool,
}

#[derive(Default)]
struct CadenceState {
    last_timestamp: Option<std::time::SystemTime>,
    crank_revolutions: f64,
}

#[interface(name = "org.bluez.GattCharacteristic1")]
impl MeasurementCharacteristic {
    #[zbus(name = "ReadValue")]
    fn read_value(&self, _options: HashMap<String, OwnedValue>) -> Vec<u8> {
        self.value()
    }

    #[zbus(name = "StartNotify")]
    fn start_notify(&mut self) {
        self.notifying = true;
        log::info(
            "app.gatt",
            "subscribed",
            &[("char", "measurement".to_string())],
        );
    }

    #[zbus(name = "StopNotify")]
    fn stop_notify(&mut self) {
        self.notifying = false;
        log::info(
            "app.gatt",
            "unsubscribed",
            &[("char", "measurement".to_string())],
        );
    }

    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        MEASUREMENT_UUID
    }

    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(CPS_SERVICE_PATH).expect("static object path")
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        vec!["read", "notify"]
    }

    #[zbus(property, name = "Value")]
    fn value(&self) -> Vec<u8> {
        cycling_power_measurement(
            self.power_watts,
            self.crank_revolutions,
            self.crank_event_time,
        )
    }

    #[zbus(property)]
    fn notifying(&self) -> bool {
        self.notifying
    }
}

impl MeasurementCharacteristic {
    fn apply_measurement(&mut self, measurement: &Measurement, cadence: &mut CadenceState) {
        self.power_watts = measurement.power_watts as i16;

        if let Some(last_timestamp) = cadence.last_timestamp {
            if measurement.cadence_rpm > 0 {
                let elapsed = measurement
                    .timestamp
                    .duration_since(last_timestamp)
                    .unwrap_or_default()
                    .as_secs_f64();
                cadence.crank_revolutions += f64::from(measurement.cadence_rpm) * elapsed / 60.0;
                let event_delta = (elapsed * 1024.0).round() as u16;
                self.crank_event_time = self.crank_event_time.wrapping_add(event_delta);
            }
        }

        cadence.last_timestamp = Some(measurement.timestamp);
        self.crank_revolutions = cadence.crank_revolutions as u16;
    }
}
struct FeatureCharacteristic;

#[interface(name = "org.bluez.GattCharacteristic1")]
impl FeatureCharacteristic {
    #[zbus(name = "ReadValue")]
    fn read_value(&self, _options: HashMap<String, OwnedValue>) -> Vec<u8> {
        0u32.to_le_bytes().to_vec()
    }

    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        FEATURE_UUID
    }

    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(CPS_SERVICE_PATH).expect("static object path")
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        vec!["read"]
    }
}

struct SensorLocationCharacteristic;

#[interface(name = "org.bluez.GattCharacteristic1")]
impl SensorLocationCharacteristic {
    #[zbus(name = "ReadValue")]
    fn read_value(&self, _options: HashMap<String, OwnedValue>) -> Vec<u8> {
        vec![0]
    }

    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        SENSOR_LOCATION_UUID
    }

    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(CPS_SERVICE_PATH).expect("static object path")
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        vec!["read"]
    }
}

fn cycling_power_measurement(
    power_watts: i16,
    crank_revolutions: u16,
    crank_event_time: u16,
) -> Vec<u8> {
    let mut value = Vec::with_capacity(8);
    value.extend_from_slice(&0x20u16.to_le_bytes());
    value.extend_from_slice(&power_watts.to_le_bytes());
    value.extend_from_slice(&crank_revolutions.to_le_bytes());
    value.extend_from_slice(&crank_event_time.to_le_bytes());
    value
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{cycling_power_measurement, CadenceState, MeasurementCharacteristic};
    use crate::telemetry::Measurement;

    #[test]
    fn measurement_includes_crank_revolution_data() {
        assert_eq!(
            cycling_power_measurement(250, 12, 2048),
            vec![0x20, 0x00, 0xfa, 0x00, 0x0c, 0x00, 0x00, 0x08]
        );
    }

    #[test]
    fn cadence_updates_crank_revolutions_and_event_time() {
        let mut characteristic = MeasurementCharacteristic::default();
        let mut cadence = CadenceState::default();
        let mut first = Measurement::live(120, 60);
        first.timestamp = UNIX_EPOCH;
        let mut second = Measurement::live(121, 60);
        second.timestamp = UNIX_EPOCH + Duration::from_secs(2);

        characteristic.apply_measurement(&first, &mut cadence);
        characteristic.apply_measurement(&second, &mut cadence);

        assert_eq!(characteristic.power_watts, 121);
        assert_eq!(characteristic.crank_revolutions, 2);
        assert_eq!(characteristic.crank_event_time, 2048);
    }
}
