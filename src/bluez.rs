use std::collections::HashMap;

use zbus::blocking::{Connection, Proxy};
use zvariant::{OwnedValue, Value};

use crate::adapter::AdapterId;
use crate::error::PedalcastError;
use crate::log;

pub fn configure_adapter(adapter: AdapterId, name: &str) -> Result<(), PedalcastError> {
    let connection = Connection::system().map_err(|source| {
        PedalcastError::runtime(format!("D-Bus connect for adapter config failed: {source}"))
    })?;
    let adapter_path = format!("/org/bluez/{adapter}");
    let properties = Proxy::new(
        &connection,
        "org.bluez",
        adapter_path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .map_err(|source| {
        PedalcastError::runtime(format!("adapter properties proxy failed: {source}"))
    })?;

    let adapter_interface = "org.bluez.Adapter1";
    properties
        .call::<_, _, ()>("Set", &(adapter_interface, "Alias", Value::from(name)))
        .map_err(|source| {
            PedalcastError::runtime(format!("failed to set adapter alias: {source}"))
        })?;
    properties
        .call::<_, _, ()>(
            "Set",
            &(adapter_interface, "Pairable", OwnedValue::from(false)),
        )
        .map_err(|source| {
            PedalcastError::runtime(format!("failed to disable pairing: {source}"))
        })?;
    properties
        .call::<_, _, ()>(
            "Set",
            &(adapter_interface, "Discoverable", OwnedValue::from(false)),
        )
        .map_err(|source| {
            PedalcastError::runtime(format!("failed to disable classic discovery: {source}"))
        })?;

    let options: HashMap<&str, OwnedValue> = HashMap::new();
    let adapter_proxy = Proxy::new(
        &connection,
        "org.bluez",
        adapter_path.as_str(),
        "org.bluez.Adapter1",
    )
    .map_err(|source| PedalcastError::runtime(format!("adapter proxy failed: {source}")))?;
    let _ = adapter_proxy.call::<_, _, ()>("StopDiscovery", &options);

    log::info(
        "adapter.server",
        "configured",
        &[
            ("adapter", adapter.to_string()),
            ("alias", name.to_string()),
            ("pairable", "false".to_string()),
            ("discoverable", "false".to_string()),
        ],
    );
    Ok(())
}
