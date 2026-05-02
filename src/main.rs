mod adapter;
mod ble;
mod config;
mod error;
mod keiser;
mod log;
mod supervisor;
mod telemetry;

use std::env;
use std::process::ExitCode;

use adapter::AdapterRegistry;
use config::Config;
use error::PedalcastError;
use supervisor::{RunMode, Supervisor};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(PedalcastError::SuccessExit) => ExitCode::SUCCESS,
        Err(error) => {
            log::error("daemon", "startup_failed", &[("error", error.to_string())]);
            ExitCode::from(error.exit_code())
        }
    }
}

fn run() -> Result<(), PedalcastError> {
    let args = Args::parse(env::args().skip(1))?;
    let config = Config::from_path(&args.config_path)?;
    let registry = AdapterRegistry::detect()?;

    log::info(
        "config",
        "loaded",
        &[
            ("path", args.config_path.clone()),
            ("bike_adapter", config.bike.adapter.to_string()),
            ("server_adapter", config.server.adapter.to_string()),
        ],
    );

    let supervisor = Supervisor::new(config, registry, args.allow_single_adapter)?;
    supervisor.run(args.run_mode)
}

struct Args {
    config_path: String,
    allow_single_adapter: bool,
    run_mode: RunMode,
}

impl Args {
    fn parse<I>(mut args: I) -> Result<Self, PedalcastError>
    where
        I: Iterator<Item = String>,
    {
        let mut config_path = "/etc/pedalcast/config.toml".to_string();
        let mut allow_single_adapter = false;
        let mut run_mode = RunMode::Daemon;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    config_path = args
                        .next()
                        .ok_or_else(|| PedalcastError::usage("--config requires a path"))?;
                }
                "--allow-single-adapter" => allow_single_adapter = true,
                "--check" => run_mode = RunMode::Check,
                "--help" | "-h" => {
                    print_help();
                    return Err(PedalcastError::success_exit());
                }
                unknown => {
                    return Err(PedalcastError::usage(format!(
                        "unknown argument `{unknown}`"
                    )));
                }
            }
        }

        Ok(Self {
            config_path,
            allow_single_adapter,
            run_mode,
        })
    }
}

fn print_help() {
    println!("pedalcast\n\nusage: pedalcast [--config PATH] [--check] [--allow-single-adapter]\n");
}
