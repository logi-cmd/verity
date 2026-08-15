# Verity

Verity is an open-source local tool that answers one bounded question: **can this trusted but unfamiliar repository run on this machine, under the recorded environment?**

It detects supported project targets, shows the exact evidence behind every planned command, executes an isolated repository snapshot, and signs a local verification receipt only when the target's machine oracle passes. It does not infer product purpose, score delivery readiness, manage plugins, or claim that an unknown repository is safe.

## Implemented adapters (pre-beta)

- Node.js
- Deno
- Bun
- Static web sites
- Rust
- Python
- Go
- Godot (native confirmation required)
- Docker Compose products
- Java and Kotlin through Maven or a checked-in Gradle Wrapper
- C and C++ through CMake, Meson, or Make (native confirmation required)
- .NET through locked NuGet restore
- PHP through Composer
- Ruby through Bundler

Every logical target has three independent states: plan completeness, current-machine environment compatibility, and oracle strength. Product targets are shown first; workspace members, fixtures, examples, libraries, and tools remain available as advanced components and are never presented as separate products by default. A running process, open port, or visible window is not enough for `verified`.

After a `verified` run, Verity can inspect deterministic cleanup candidates in the same task. Exact duplicate and residue detection is always available. Stack-specific analysis uses Knip, cargo-machete, Vulture, and Go deadcode when the corresponding analyzer is available; the UI reports whether each analyzer completed, was not installed, was unsafe to promote, failed, or did not apply. Analyzer findings remain report-only unless the finding is a file-level candidate with an explicit entry configuration and Verity can rerun the unchanged baseline oracle.

Verity removes candidates only inside an isolated snapshot, reruns the same oracle, and labels a group `removal_verified` only when the result remains `verified`. Tests, migrations, CI, deployment files, licenses, documentation, schemas, and public API surfaces stay report-only. A `started_unverified` run can receive a candidate report but cannot produce a write-back action.

## CLI

```powershell
cargo run -p verity-cli -- inspect C:\path\to\repository --json
cargo run -p verity-cli -- check C:\path\to\repository --target node-0123456789
cargo run -p verity-cli -- receipt SESSION_ID
cargo run -p verity-cli -- verify-receipt C:\path\to\receipt.json --repository C:\path\to\repository --json
cargo run -p verity-cli -- runtime doctor
```

Human-readable `inspect` output includes the target ID, relative path, role, selection reason, blocker source, and a copyable `check --target` command.

## Desktop

```powershell
npm --prefix desktop install
npm run dev:desktop
```

The desktop app has three top-level destinations: Current check, History, and Settings. No account is required. Verity has no telemetry transport. A local allowlisted diagnostic report can be previewed and saved manually; nothing is uploaded automatically.

## Development

```powershell
cargo test --workspace
npm run test:contracts
npm run build:desktop-web
```

Verity is beta software. Current release evidence is recorded in [docs/release-status.md](docs/release-status.md). The local signature proves that a receipt has not changed since this installation signed it; it is not a remote Verity certification.

## License

MPL-2.0. A future team control plane is outside this local open-source workspace. Raw receipts, paths, logs, source, and command output remain local; this repository contains no upload interface.
