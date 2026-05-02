use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

use crate::adapter::AdapterId;
use crate::error::PedalcastError;
use crate::keiser;
use crate::log;
use crate::telemetry::DropoutFilter;

pub struct BtmonScanner {
    adapter: AdapterId,
    suppress_single_zero_dropouts: bool,
    telemetry_tx: Option<Sender<i16>>,
    btmon: Option<Child>,
}

impl BtmonScanner {
    pub fn new(
        adapter: AdapterId,
        suppress_single_zero_dropouts: bool,
        telemetry_tx: Option<Sender<i16>>,
    ) -> Self {
        Self {
            adapter,
            suppress_single_zero_dropouts,
            telemetry_tx,
            btmon: None,
        }
    }

    pub fn start(&mut self) -> Result<(), PedalcastError> {
        let adapter = self.adapter.to_string();
        let mut btmon = Command::new("sudo")
            .args(["btmon", "-i", &adapter])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| {
                PedalcastError::runtime(format!("failed to start btmon: {source}"))
            })?;

        let stdout = btmon
            .stdout
            .take()
            .ok_or_else(|| PedalcastError::runtime("failed to capture btmon stdout"))?;
        let suppress = self.suppress_single_zero_dropouts;
        let telemetry_tx = self.telemetry_tx.clone();
        thread::spawn(move || parse_btmon(stdout, suppress, telemetry_tx));

        let _ = run_command(
            "sudo",
            &[
                "hcitool", "-i", &adapter, "cmd", "0x08", "0x000C", "00", "00",
            ],
        );
        run_command(
            "sudo",
            &[
                "hcitool", "-i", &adapter, "cmd", "0x08", "0x000B", "01", "10", "00", "10", "00",
                "00", "00",
            ],
        )?;
        run_command(
            "sudo",
            &[
                "hcitool", "-i", &adapter, "cmd", "0x08", "0x000C", "01", "00",
            ],
        )?;

        log::info(
            "bike.keiser",
            "scan_started",
            &[("adapter", self.adapter.to_string())],
        );
        self.btmon = Some(btmon);
        Ok(())
    }
}
impl Drop for BtmonScanner {
    fn drop(&mut self) {
        let adapter = self.adapter.to_string();
        let _ = run_command(
            "sudo",
            &[
                "hcitool", "-i", &adapter, "cmd", "0x08", "0x000C", "00", "00",
            ],
        );
        if let Some(child) = &mut self.btmon {
            let _ = child.kill();
        }
    }
}

fn parse_btmon(
    stdout: impl std::io::Read,
    suppress_single_zero_dropouts: bool,
    telemetry_tx: Option<Sender<i16>>,
) {
    let reader = BufReader::new(stdout);
    let mut current_address = String::new();
    let mut dropout_filter = DropoutFilter::new(suppress_single_zero_dropouts);

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if let Some(address) = trimmed.strip_prefix("Address: ") {
            current_address = address
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
        } else if let Some(name) = trimmed.strip_prefix("Name (complete): ") {
            if name.contains("M3") {
                log::info(
                    "bike.keiser",
                    "candidate_name",
                    &[
                        ("address", current_address.clone()),
                        ("name", name.to_string()),
                    ],
                );
            }
        } else if let Some(data) = trimmed.strip_prefix("Data: ") {
            let Some(payload) = decode_hex(data) else {
                continue;
            };
            if payload.len() < 12 || payload[0..2] != [0x02, 0x01] {
                continue;
            }

            match keiser::parse_stats(&payload) {
                Ok(stats) => {
                    let version = keiser::bike_version(&payload).ok();
                    for measurement in dropout_filter.ingest(stats.into_measurement()) {
                        if let Some(telemetry_tx) = &telemetry_tx {
                            let _ = telemetry_tx.send(measurement.power_watts as i16);
                        }
                        log::info(
                            "bike.keiser",
                            "telemetry",
                            &[
                                ("address", current_address.clone()),
                                ("power_watts", measurement.power_watts.to_string()),
                                ("cadence_rpm", measurement.cadence_rpm.to_string()),
                                ("quality", format!("{:?}", measurement.source_quality)),
                            ],
                        );
                    }
                    if let Some(version) = version {
                        log::info(
                            "bike.keiser",
                            "version",
                            &[
                                ("address", current_address.clone()),
                                (
                                    "version",
                                    format!("{:x}.{:x}", version.major, version.minor),
                                ),
                                ("stats_timeout_ms", version.stats_timeout_ms.to_string()),
                            ],
                        );
                    }
                }
                Err(error) => {
                    log::warn(
                        "bike.keiser",
                        "parse_failed",
                        &[
                            ("address", current_address.clone()),
                            ("error", error.to_string()),
                        ],
                    );
                }
            }
        }
    }
}

fn run_command(program: &str, args: &[&str]) -> Result<(), PedalcastError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|source| PedalcastError::runtime(format!("failed to run {program}: {source}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(PedalcastError::runtime(format!(
        "{program} {} failed: {}{}",
        args.join(" "),
        stderr.trim(),
        stdout.trim()
    )))
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let hex = value.trim();
    if hex.len() % 2 != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for index in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[index..index + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}
