use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::adapter::AdapterRegistry;
use crate::advertise::BluezAdvertiser;
use crate::ble::BtmonScanner;
use crate::bluez::configure_adapter;
use crate::config::Config;
use crate::error::PedalcastError;
use crate::gatt::CyclingPowerGatt;
use crate::log;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMode {
    Check,
    Daemon,
}
pub struct Supervisor {
    config: Config,
}

impl Supervisor {
    pub fn new(
        config: Config,
        registry: AdapterRegistry,
        allow_single_adapter: bool,
    ) -> Result<Self, PedalcastError> {
        registry.require("bike", config.bike.adapter)?;
        registry.require("server", config.server.adapter)?;

        if config.bike.adapter == config.server.adapter && !allow_single_adapter {
            return Err(PedalcastError::adapter(format!(
                "bike and server both use {}; configure separate adapters or pass --allow-single-adapter for debugging",
                config.bike.adapter
            )));
        }

        log::info(
            "adapter.bike",
            "selected",
            &[
                ("adapter", config.bike.adapter.to_string()),
                ("role", "scan".to_string()),
            ],
        );
        log::info(
            "adapter.server",
            "selected",
            &[
                ("adapter", config.server.adapter.to_string()),
                ("role", "gatt".to_string()),
            ],
        );

        Ok(Self { config })
    }

    pub fn run(self, mode: RunMode) -> Result<(), PedalcastError> {
        match mode {
            RunMode::Check => {
                log::info("supervisor", "config_ok", &[]);
                Ok(())
            }
            RunMode::Daemon => self.run_daemon(),
        }
    }

    fn run_daemon(self) -> Result<(), PedalcastError> {
        configure_adapter(self.config.server.adapter, &self.config.server.name)?;
        let (telemetry_tx, telemetry_rx) = mpsc::channel();
        CyclingPowerGatt::new(self.config.server.adapter, telemetry_rx).start();
        BluezAdvertiser::new(self.config.server.adapter, self.config.server.name.clone()).start();

        let mut scanner = BtmonScanner::new(
            self.config.bike.adapter,
            self.config.filter.suppress_single_zero_dropouts,
            Some(telemetry_tx),
        );
        scanner.start()?;

        log::info(
            "supervisor",
            "running",
            &[
                ("bike", "searching".to_string()),
                ("app_server", "advertising_gatt_registering".to_string()),
            ],
        );

        loop {
            thread::sleep(Duration::from_secs(60));
            log::info(
                "supervisor",
                "heartbeat",
                &[
                    ("bike", "searching".to_string()),
                    ("app_server", "advertising_gatt_registering".to_string()),
                ],
            );
        }
    }
}
