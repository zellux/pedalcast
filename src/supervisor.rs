use std::thread;
use std::time::Duration;

use crate::adapter::AdapterRegistry;
use crate::config::Config;
use crate::error::PedalcastError;
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
        log::info(
            "bike.keiser",
            "scanner_pending_bluez",
            &[("adapter", self.config.bike.adapter.to_string())],
        );
        log::warn(
            "app.ble",
            "gatt_server_pending_bluez",
            &[
                ("adapter", self.config.server.adapter.to_string()),
                ("name", self.config.server.name.clone()),
            ],
        );
        log::info(
            "supervisor",
            "running",
            &[
                ("bike", "searching".to_string()),
                ("app_server", "not_implemented".to_string()),
            ],
        );

        loop {
            thread::sleep(Duration::from_secs(60));
            log::info(
                "supervisor",
                "heartbeat",
                &[
                    ("bike", "searching".to_string()),
                    ("app_server", "not_implemented".to_string()),
                ],
            );
        }
    }
}
