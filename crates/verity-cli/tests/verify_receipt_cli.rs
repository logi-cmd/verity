// SPDX-License-Identifier: MPL-2.0

use std::process::Command;

#[test]
fn missing_receipt_file_is_an_operational_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_verity"))
        .args([
            "verify-receipt",
            "definitely-missing-receipt.json",
            "--repository",
            ".",
            "--json",
        ])
        .output()
        .expect("verity CLI should start");

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("verity:"));
    assert!(output.stdout.is_empty());
}

#[test]
fn repository_and_json_flags_are_part_of_the_cli_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_verity"))
        .args(["verify-receipt", "receipt.json"])
        .output()
        .expect("verity CLI should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--repository"));
}
