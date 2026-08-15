# Security model

Verity is for repositories whose source is trusted but unfamiliar. It is not an adversarial-code sandbox.

- The selected source repository is read-only until a separately reviewed patch write-back.
- Symbolic links, secret-like files, generated output, dependency directories, and VCS metadata are excluded or rejected during snapshot creation.
- Container phases have CPU, memory, PID, and network controls. Dependency lifecycle scripts are deferred until the offline phase.
- Native execution requires explicit desktop confirmation and provides process containment, not hostile-code isolation.
- Secret values stay in the operating-system credential store and are omitted from logs, receipts, local diagnostic reports, and Agent task packs.
- Local diagnostic reports use a fixed field allowlist and never contain repository identity, paths, commands, log text, source, credentials, prompts, or patches. Verity does not upload them.
- Desktop Agent applications may be detected and opened for task handoff, but Verity does not automate their windows. A CLI may run only after its exact entry, version, and confinement contract pass an explicit capability test.
- Agent output is checked against pre-run file hashes, applied to a second clean snapshot, and cannot change verification status; only a fresh deterministic rerun can. A verified diff is shown before write-back, and write-back is refused if any original file hash changed.
