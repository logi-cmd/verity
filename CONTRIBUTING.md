# Contributing to Verity

Verity accepts focused fixes and adapter improvements that preserve its bounded claim: a target is `verified` only after the recorded machine oracle passes.

Before opening a pull request:

```powershell
cargo fmt --all -- --check
cargo test --workspace
npm --prefix desktop ci
npm run test:contracts
npm run build:desktop-web
```

Keep the local core self-contained: no telemetry or network transport for repository data. New commands must come from repository evidence, secrets must be redacted, and missing evidence must block rather than guess.

By contributing, you agree that your contribution is licensed under MPL-2.0.
