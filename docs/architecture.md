# Architecture

The workspace has four Rust layers:

- `verity-core`: Git-aware repository snapshots, logical targets, plans, sessions, verification/cleanup receipts, and versioned schemas.
- `verity-adapters`: deterministic component discovery and product-target composition for Node, Deno, Bun, static web, Rust, Python, Go, Godot, Tauri, Compose, Java/Kotlin, C/C++, .NET, PHP, and Ruby.
- `verity-runner`: target-specific environment checks, constrained container/native execution, Agent discovery/confinement, phase supervision, machine oracles, verified cleanup, redaction, allowlisted local diagnostics, and local signing.
- `verity-cli`: headless inspection, checking, receipt access, receipt verification, and runtime diagnostics.
- `verity-desktop`: a Tauri interaction shell over the same core and runner.

Adapters first build a component graph, then expose logical product targets. Commands carry a per-command relative working directory so cross-workspace and composite Tauri plans do not run from the wrong folder. The runner copies Git-tracked and non-ignored source to the Verity application-data directory, compares source and snapshot fingerprints, performs dependency acquisition before network removal, and stops after the first observed failing phase. UI state never promotes a result.

Cleanup uses a verified receipt as its baseline. The preview is a versioned object containing both candidates and the explicit state of Knip, cargo-machete, Vulture, and Go deadcode. Missing or failed analyzers cannot be mistaken for a clean result. Analyzer output is parsed into report-only candidates by default; only eligible file deletions move into the existing remove-and-reverify pipeline. Candidates are deleted only in a fresh snapshot, tested in groups, and split when a group fails. Write-back is an explicit operation that rechecks every original file hash; any overlap with later user edits stops the operation.

Docker Desktop is the only container runtime implemented in this release. Agent CLI capability is bound to an exact entry hash and version; desktop Agent applications are handoff surfaces only. Diagnostic reports are built from aggregate allowlisted fields and have no transport.
