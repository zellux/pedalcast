use std::collections::HashMap;
use std::thread;

use zbus::blocking::{Connection, Proxy};
use zbus::interface;
use zvariant::{ObjectPath, OwnedValue};

use crate::adapter::AdapterId;
use crate::error::PedalcastError;
use crate::log;

const ADVERTISEMENT_PATH: &str = "/com/pedalcast/advertisement0";
const CPS_UUID: &str = "00001818-0000-1000-8000-00805f9b34fb";

pub struct BluezAdvertiser {
    adapter: AdapterId,
    name: String,
}

impl BluezAdvertiser {
    pub fn new(adapter: AdapterId, name: String) -> Self {
        Self { adapter, name }
    }

    pub fn start(self) {
        thread::spawn(move || {
            if let Err(error) = self.run() {
                log::error(
                    "app.ble",
                    "advertising_failed",
                    &[("error", error.to_string())],
                );
            }
        });
    }

    fn run(self) -> Result<(), PedalcastError> {
        let connection = Connection::system().map_err(|source| {
            PedalcastError::runtime(format!("D-Bus connect for advertising failed: {source}"))
        })?;
        connection
            .object_server()
            .at(ADVERTISEMENT_PATH, Advertisement::new(self.name.clone()))
            .map_err(|source| {
                PedalcastError::runtime(format!("advertisement export failed: {source}"))
            })?;

        let adapter_path = format!("/org/bluez/{}", self.adapter);
        let manager = Proxy::new(
            &connection,
            "org.bluez",
            adapter_path.as_str(),
            "org.bluez.LEAdvertisingManager1",
        )
        .map_err(|source| {
            PedalcastError::runtime(format!("LEAdvertisingManager proxy failed: {source}"))
        })?;

        let options: HashMap<&str, OwnedValue> = HashMap::new();
        let advertisement_path = ObjectPath::try_from(ADVERTISEMENT_PATH).map_err(|source| {
            PedalcastError::runtime(format!("invalid advertisement path: {source}"))
        })?;
        manager
            .call::<_, _, ()>("RegisterAdvertisement", &(advertisement_path, options))
            .map_err(|source| {
                PedalcastError::runtime(format!("RegisterAdvertisement failed: {source}"))
            })?;

        log::info(
            "app.ble",
            "advertising_registered",
            &[
                ("adapter", self.adapter.to_string()),
                ("name", self.name),
                ("service", "cycling_power".to_string()),
            ],
        );

        loop {
            thread::park();
        }
    }
}
struct Advertisement {
    name: String,
}

impl Advertisement {
    fn new(name: String) -> Self {
        Self { name }
    }
}

#[interface(name = "org.bluez.LEAdvertisement1")]
impl Advertisement {
    fn release(&self) {
        log::info("app.ble", "advertisement_released", &[]);
    }

    #[zbus(property, name = "Type")]
    fn advertisement_type(&self) -> &str {
        "peripheral"
    }

    #[zbus(property, name = "ServiceUUIDs")]
    fn service_uuids(&self) -> Vec<&str> {
        vec![CPS_UUID]
    }

    #[zbus(property, name = "LocalName")]
    fn local_name(&self) -> &str {
        &self.name
    }
}
