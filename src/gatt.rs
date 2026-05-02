use std::collections::HashMap;
use std::thread;

use zbus::blocking::{Connection, Proxy};
use zbus::fdo::ObjectManager;
use zbus::interface;
use zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

use crate::adapter::AdapterId;
use crate::error::PedalcastError;
use crate::log;

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
}

impl CyclingPowerGatt {
    pub fn new(adapter: AdapterId) -> Self {
        Self { adapter }
    }

    pub fn start(self) {
        thread::spawn(move || {
            if let Err(error) = self.run() {
                log::error("app.gatt", "failed", &[("error", error.to_string())]);
            }
        });
    }

    fn run(self) -> Result<(), PedalcastError> {
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

        loop {
            thread::park();
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
    notifying: bool,
}

#[interface(name = "org.bluez.GattCharacteristic1")]
impl MeasurementCharacteristic {
    #[zbus(name = "ReadValue")]
    fn read_value(&self, _options: HashMap<String, OwnedValue>) -> Vec<u8> {
        cycling_power_measurement(0)
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

    #[zbus(property)]
    fn notifying(&self) -> bool {
        self.notifying
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

fn cycling_power_measurement(power_watts: i16) -> Vec<u8> {
    let mut value = Vec::with_capacity(4);
    value.extend_from_slice(&0u16.to_le_bytes());
    value.extend_from_slice(&power_watts.to_le_bytes());
    value
}
