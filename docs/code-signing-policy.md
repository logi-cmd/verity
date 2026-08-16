# Code signing policy

Verity does not currently publish Windows installers. An installer will appear on the official download page only after its Authenticode signature, timestamp, installation, uninstallation, launch, and release evidence have all been checked.

## Planned signing provider

Application pending: **Free code signing provided by [SignPath.io](https://signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).**

This names the provider Verity is applying to use. It does not mean the application has been accepted or that any current Verity build is signed. When signing is available, Windows will show `SignPath Foundation` as the publisher.

## Roles

- Author, committer, and reviewer: [logi-cmd](https://github.com/logi-cmd)
- Signing approver: [logi-cmd](https://github.com/logi-cmd)

Verity currently has one maintainer. Contributions from other people must be reviewed before they are merged. Every signing request requires a separate manual approval.

## Release rules

- Release candidates are built from a public Verity tag on GitHub-hosted runners.
- The signing service must be able to connect the submitted files to that workflow run and source tag.
- The Verity application executable and every published MSI or setup executable must pass `signtool verify /pa /all`.
- Published signatures must include a trusted timestamp.
- Installation, uninstallation, launch, and the Windows acceptance matrix must pass.
- Unsigned installers are not uploaded to GitHub Releases or linked from the official download page.
- SHA-256 checksums, a release manifest, and an acceptance report are published with each installer release.

## Privacy

Verity has no telemetry or upload transport. It does not transfer repository source, paths, logs, command output, or complete receipts to another networked system. A repository command chosen by the user may use the network according to that repository's behavior; Verity shows the plan and asks for consent before it runs. See the [privacy page](https://agent-guardrails.com/privacy/) for the current data boundary.

Last updated: August 17, 2026.
