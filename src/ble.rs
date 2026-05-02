use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::thread;

use crate::adapter::AdapterId;
use crate::error::PedalcastError;
use crate::keiser;
use crate::log;
use crate::telemetry::DropoutFilter;

pub struct LegacyAdvertiser {
    adapter: AdapterId,
    name: String,
}

impl LegacyAdvertiser {
    pub fn new(adapter: AdapterId, name: String) -> Self {
        Self { adapter, name }
    }

    pub fn start(&self) -> Result<(), PedalcastError> {
        let adapter = self.adapter.to_string();
        run_command("sudo", &["hciconfig", &adapter, "up"])?;
        run_command("sudo", &["hciconfig", &adapter, "name", &self.name])?;
        let _ = run_command("sudo", &["hciconfig", &adapter, "noleadv"]);

        let hex_values = advertising_payload(&self.name);
        let mut args = vec!["hcitool", "-i", &adapter, "cmd", "0x08", "0x0008"];
        for value in &hex_values {
            args.push(value);
        }
        run_command("sudo", &args)?;
        run_command("sudo", &["hciconfig", &adapter, "leadv", "0"])?;
        run_command("sudo", &args)?;

        log::info(
            "app.ble",
            "legacy_advertising",
            &[
                ("adapter", self.adapter.to_string()),
                ("name", self.name.clone()),
                ("service", "cycling_power".to_string()),
            ],
        );
        Ok(())
    }
}

pub struct BtmonScanner {
    adapter: AdapterId,
    suppress_single_zero_dropouts: bool,
    btmon: Option<Child>,
}

impl BtmonScanner {
    pub fn new(adapter: AdapterId, suppress_single_zero_dropouts: bool) -> Self {
        Self {
            adapter,
            suppress_single_zero_dropouts,
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
        thread::spawn(move || parse_btmon(stdout, suppress));

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

fn parse_btmon(stdout: impl std::io::Read, suppress_single_zero_dropouts: bool) {
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

fn advertising_payload(name: &str) -> Vec<String> {
    let mut data = Vec::new();
    data.push(0x02);
    data.push(0x01);
    data.push(0x06);
    data.push(0x03);
    data.push(0x03);
    data.push(0x18);
    data.push(0x18);

    let name_bytes = name.as_bytes();
    let max_name_bytes = 29usize.saturating_sub(data.len());
    let name_bytes = &name_bytes[..name_bytes.len().min(max_name_bytes)];
    data.push((name_bytes.len() + 1) as u8);
    data.push(0x09);
    data.extend_from_slice(name_bytes);

    let length = data.len() as u8;
    let mut payload = vec![format!("{length:02X}")];
    payload.extend(data.iter().map(|byte| format!("{byte:02X}")));
    while payload.len() < 32 {
        payload.push("00".to_string());
    }
    payload
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
