// SPDX-License-Identifier: MPL-2.0

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Parser)]
#[command(
    name = "verity",
    version,
    about = "Prove whether a trusted-source repository can run in a recorded local environment."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Inspect {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Check {
        path: PathBuf,
        #[arg(long)]
        target: Option<String>,
    },
    Receipt {
        session_id: String,
    },
    VerifyReceipt {
        file: PathBuf,
        #[arg(long)]
        repository: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    Agents,
    Diagnostic,
}

#[derive(Subcommand)]
enum RuntimeCommand {
    Doctor,
}

fn print_json<T: serde::Serialize>(value: &T) {
    println!("{}", serde_json::to_string_pretty(value).unwrap());
}

fn print_progress(event: verity_core::RunProgressEvent) {
    if event.kind == verity_core::RunProgressEventKind::Observation {
        return;
    }
    let detail = event
        .observation
        .as_ref()
        .map(|observation| observation.text.as_str())
        .unwrap_or(&event.message);
    eprintln!("[{:?}] {detail}", event.progress.phase);
}

fn main() {
    let result = run();
    if let Err(error) = result {
        eprintln!("verity: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Commands::Inspect { path, json } => {
            let mut plan = verity_adapters::inspect_repository(&path)?;
            verity_runner::assess_plan_environment(&mut plan);
            if json {
                print_json(&plan);
            } else {
                println!(
                    "{} target(s) found in {}",
                    plan.targets.len(),
                    plan.repository_name
                );
                for target in plan.targets {
                    let path = if target.relative_root.is_empty() {
                        "."
                    } else {
                        &target.relative_root
                    };
                    println!(
                        "- {} [{}] [{:?}/{:?}/{:?}] plan={:?} environment={:?} oracle={:?}",
                        target.label,
                        target.id,
                        target.stack,
                        target.kind,
                        target.role,
                        target.plan_status,
                        target.environment_status,
                        target.oracle_status
                    );
                    println!("  path: {path}");
                    println!("  selection: {}", target.selection_reason);
                    println!(
                        "  check: verity check \"{}\" --target {}",
                        plan.repository_root, target.id
                    );
                    for blocker in target.blockers {
                        println!(
                            "  blocker [{:?}/{:?}]: {}",
                            blocker.origin, blocker.phase, blocker.summary
                        );
                    }
                }
            }
        }
        Commands::Check { path, target } => {
            let mut plan = verity_adapters::inspect_repository(&path)?;
            verity_runner::assess_plan_environment(&mut plan);
            let target_id = target
                .or_else(|| {
                    let recommended = plan
                        .targets
                        .iter()
                        .filter(|item| {
                            item.recommended
                                && item.plan_status == verity_core::PlanStatus::Complete
                        })
                        .collect::<Vec<_>>();
                    (recommended.len() == 1).then(|| recommended[0].id.clone())
                })
                .ok_or(
                    "no unique recommended product target; run `verity inspect` and pass --target",
                )?;
            let session = uuid::Uuid::new_v4().to_string();
            let selected = plan
                .targets
                .iter()
                .find(|item| item.id == target_id)
                .ok_or("selected target does not exist")?;
            let receipt = if selected.commands.iter().any(|command| command.native) {
                verity_runner::execute_target_confirmed_native(
                    &plan,
                    &target_id,
                    &session,
                    &AtomicBool::new(false),
                    print_progress,
                )?
            } else {
                verity_runner::execute_target(
                    &plan,
                    &target_id,
                    &session,
                    &AtomicBool::new(false),
                    print_progress,
                )?
            };
            print_json(&receipt);
            match receipt.result {
                verity_core::TargetResult::Verified => {}
                verity_core::TargetResult::StartedUnverified => std::process::exit(3),
                _ => std::process::exit(2),
            }
        }
        Commands::Receipt { session_id } => {
            let receipt = verity_runner::list_receipts()?
                .into_iter()
                .find(|item| item.session_id == session_id)
                .ok_or("receipt not found")?;
            print_json(&receipt);
        }
        Commands::VerifyReceipt {
            file,
            repository,
            json,
        } => {
            let receipt: verity_core::VerificationReceipt =
                serde_json::from_slice(&std::fs::read(file)?)?;
            let verification = verity_runner::verify_receipt_for_repository(&receipt, &repository);
            if json {
                print_json(&verification);
            } else {
                println!("{}", verification.reason_code);
            }
            if !verification.accepted {
                std::process::exit(2);
            }
        }
        Commands::Runtime {
            command: RuntimeCommand::Doctor,
        } => print_json(&verity_runner::runtime_doctor()),
        Commands::Agents => print_json(&verity_runner::agent_capabilities()),
        Commands::Diagnostic => print_json(&verity_runner::diagnostic_report()),
    }
    Ok(())
}
