// SPDX-License-Identifier: MPL-2.0

import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const model = await readFile(new URL("../crates/verity-core/src/model.rs", import.meta.url), "utf8");
const runner = await readFile(new URL("../crates/verity-runner/src/lib.rs", import.meta.url), "utf8");
const cli = await readFile(new URL("../crates/verity-cli/src/main.rs", import.meta.url), "utf8");

test("receipt verifier exposes the fixed safe machine schema", () => {
  assert.match(model, /verity-receipt-verification\.v1/);
  for (const field of [
    "receipt_id",
    "receipt_schema",
    "result",
    "signature_valid",
    "repository_fingerprint_matches",
    "snapshot_fingerprint_matches",
    "accepted",
    "reason_code",
  ]) {
    assert.match(model, new RegExp(`pub ${field}:`));
  }
  for (const unsafeField of ["path", "log", "command", "output", "phases", "oracle"]) {
    assert.doesNotMatch(model.match(/pub struct ReceiptVerification \{[\s\S]*?\n\}/)?.[0] ?? "", new RegExp(`pub ${unsafeField}:`));
  }
});

test("receipt verifier has no old-schema fallback and rejects semantic failures", () => {
  for (const reason of [
    "unsupported-schema",
    "tampered",
    "stale",
    "wrong-repository",
    "not-verified",
  ]) {
    assert.match(runner, new RegExp(`"${reason}"`));
  }
  assert.match(cli, /VerifyReceipt[\s\S]*repository: PathBuf[\s\S]*json: bool/);
  assert.match(cli, /if !verification\.accepted \{\s*std::process::exit\(2\)/);
  assert.doesNotMatch(runner, /receipt\.schema\s*==\s*"verity-verification-receipt\.v2"/);
});
