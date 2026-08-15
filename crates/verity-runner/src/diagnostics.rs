// SPDX-License-Identifier: MPL-2.0

use crate::{agent_capabilities, list_receipts, runtime_doctor};
use std::collections::BTreeMap;
use uuid::Uuid;
use verity_core::{
    DiagnosticAgentState, DiagnosticDurationBucket, DiagnosticReport, DiagnosticResultCount,
    DIAGNOSTIC_REPORT_SCHEMA,
};

fn duration_bucket(duration_ms: u64) -> &'static str {
    match duration_ms {
        0..=999 => "under_1s",
        1_000..=9_999 => "1s_to_10s",
        10_000..=59_999 => "10s_to_60s",
        60_000..=299_999 => "1m_to_5m",
        _ => "over_5m",
    }
}

pub fn diagnostic_report() -> DiagnosticReport {
    let runtime = runtime_doctor();
    let agents = agent_capabilities()
        .into_iter()
        .flat_map(|capability| {
            capability
                .installations
                .into_iter()
                .map(move |installation| DiagnosticAgentState {
                    provider: capability.provider.clone(),
                    channel: installation.channel,
                    status: installation.status,
                    reason_code: installation.reason_code,
                })
        })
        .collect();
    let receipts = list_receipts().unwrap_or_default();
    let mut results = BTreeMap::<String, u64>::new();
    let mut durations = BTreeMap::<(verity_core::RunPhase, String), u64>::new();
    for receipt in receipts {
        *results
            .entry(format!("{:?}", receipt.result).to_lowercase())
            .or_default() += 1;
        for phase in receipt.phases {
            *durations
                .entry((phase.phase, duration_bucket(phase.duration_ms).into()))
                .or_default() += 1;
        }
    }
    DiagnosticReport {
        schema: DIAGNOSTIC_REPORT_SCHEMA.into(),
        report_id: Uuid::new_v4().to_string(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        host_os: std::env::consts::OS.into(),
        host_arch: std::env::consts::ARCH.into(),
        runtime_status: runtime.status,
        runtime_reason_code: runtime.reason_code,
        agents,
        session_results: results
            .into_iter()
            .map(|(result, count)| DiagnosticResultCount { result, count })
            .collect(),
        phase_durations: durations
            .into_iter()
            .map(|((phase, bucket), count)| DiagnosticDurationBucket {
                phase,
                bucket,
                count,
            })
            .collect(),
        internal_error_codes: Vec::new(),
    }
}

pub fn diagnostic_json() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&diagnostic_report())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_buckets_do_not_expose_exact_timing() {
        assert_eq!(duration_bucket(12), "under_1s");
        assert_eq!(duration_bucket(1_500), "1s_to_10s");
        assert_eq!(duration_bucket(61_000), "1m_to_5m");
    }

    #[test]
    fn report_is_allowlisted_and_has_no_repository_fields() {
        let json = diagnostic_json().unwrap();
        for forbidden in [
            "repository_root",
            "repository_name",
            "repository_fingerprint",
            "inspection_fingerprint",
            "command",
            "output_excerpt",
            "absolute_path",
        ] {
            assert!(!json.contains(forbidden), "diagnostic leaked {forbidden}");
        }
        assert!(json.contains("verity-diagnostic-report.v1"));
    }
}
