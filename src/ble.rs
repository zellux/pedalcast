use std::collections::BTreeSet;
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
    let mut keiser_addresses = BTreeSet::new();
    let mut current_company_id = None;

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if let Some(address) = trimmed.strip_prefix("Address: ") {
            current_address = address
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
            current_company_id = None;
        } else if let Some(company) = trimmed.strip_prefix("Company: ") {
            current_company_id = parse_company_id(company);
        } else if let Some(name) = trimmed.strip_prefix("Name (complete): ") {
            if name.contains("M3") {
                keiser_addresses.insert(current_address.clone());
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
            let Some(payload) = normalize_keiser_payload(
                payload,
                current_company_id,
                keiser_addresses.contains(&current_address),
            ) else {
                continue;
            };

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

fn normalize_keiser_payload(
    payload: Vec<u8>,
    company_id: Option<u16>,
    known_keiser_address: bool,
) -> Option<Vec<u8>> {
    if payload.len() >= 12 && payload[0..2] == [0x02, 0x01] {
        return Some(payload);
    }

    let company_is_keiser = company_id == Some(0x0102);
    if payload.len() >= 17 && (company_is_keiser || known_keiser_address) {
        let mut prefixed = Vec::with_capacity(payload.len() + 2);
        prefixed.extend_from_slice(&[0x02, 0x01]);
        prefixed.extend_from_slice(&payload);
        return Some(prefixed);
    }

    None
}

fn parse_company_id(company: &str) -> Option<u16> {
    let start = company.rfind('(')? + 1;
    let end = company[start..].find(')')? + start;
    let value = company[start..end].trim();
    if let Some(hex) = value.strip_prefix("0x") {
        u16::from_str_radix(hex, 16).ok()
    } else {
        value.parse::<u16>().ok()
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
    let hex: String = value.chars().filter(|char| !char.is_whitespace()).collect();
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

#[cfg(test)]
mod tests {
    use super::{decode_hex, normalize_keiser_payload, parse_company_id};

    #[test]
    fn parses_decimal_and_hex_company_ids() {
        assert_eq!(parse_company_id("not assigned (258)"), Some(0x0102));
        assert_eq!(parse_company_id("not assigned (0x0102)"), Some(0x0102));
    }

    #[test]
    fn decodes_contiguous_and_spaced_hex() {
        assert_eq!(decode_hex("020106"), Some(vec![0x02, 0x01, 0x06]));
        assert_eq!(decode_hex("02 01 06"), Some(vec![0x02, 0x01, 0x06]));
    }

    #[test]
    fn normalizes_stripped_keiser_company_data() {
        let stripped = vec![
            0x06, 0x30, 0x42, 0x10, 0x08, 0x04, 0x30, 0x40, 0x80, 0x10, 0x20, 0x30, 0x40, 0x50,
            0x60, 0x70, 0x80,
        ];

        let payload = normalize_keiser_payload(stripped, Some(0x0102), false).unwrap();
        assert_eq!(payload[0..2], [0x02, 0x01]);
    }
}
