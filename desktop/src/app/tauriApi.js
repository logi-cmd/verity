// SPDX-License-Identifier: MPL-2.0

function invoke(name, payload = {}) {
  const command = globalThis.__TAURI__?.core?.invoke;
  if (!command) return Promise.reject(new Error("The native Verity runtime is unavailable in browser preview."));
  return command(name, payload);
}

export const tauriApi = {
  available: () => Boolean(globalThis.__TAURI__?.core?.invoke),
  pickRepository: () => invoke("pick_repository"),
  inspectRepository: (repoRoot) => invoke("inspect_repository", { repoRoot }),
  runtimeDoctor: () => invoke("runtime_doctor"),
  startDockerDesktop: () => invoke("start_docker_desktop"),
  createRunSession: (repoRoot, targetId) => invoke("create_run_session", { repoRoot, targetId }),
  executeRunSession: (sessionId) => invoke("execute_run_session", { sessionId, confirmed: true }),
  readRunSession: (sessionId) => invoke("read_run_session", { sessionId }),
  cancelRunSession: (sessionId) => invoke("cancel_run_session", { sessionId }),
  listReceipts: () => invoke("list_receipts"),
  readReceipt: (receiptId) => invoke("read_receipt", { receiptId }),
  verifyReceipt: (receiptId) => invoke("verify_receipt", { receiptId }),
  exportReceipt: (receiptId) => invoke("export_receipt", { receiptId }),
  previewCleanup: (repoRoot, receiptId) => invoke("preview_cleanup", { repoRoot, receiptId }),
  startCleanup: (repoRoot, receiptId, candidateIds) => invoke("start_cleanup", { repoRoot, receiptId, candidateIds }),
  readCleanupSession: (sessionId) => invoke("read_cleanup_session", { sessionId }),
  cancelCleanup: (sessionId) => invoke("cancel_cleanup", { sessionId }),
  listCleanupReceipts: (sessionId) => invoke("list_cleanup_receipts", { sessionId }),
  exportCleanupPatch: (sessionId, receiptId) => invoke("export_cleanup_patch", { sessionId, receiptId }),
  applyCleanup: (repoRoot, sessionId, receiptId) => invoke("apply_cleanup", { repoRoot, sessionId, receiptId, confirmed: true }),
  listAgentCapabilities: () => invoke("list_agent_capabilities"),
  testAgentCapability: (provider) => invoke("test_agent_capability", { provider }),
  launchAgentDesktop: (provider) => invoke("launch_agent_desktop", { provider }),
  startAgentRepair: (repoRoot, targetId, provider) => invoke("start_agent_repair", { repoRoot, targetId, provider }),
  readAgentRepair: (sessionId) => invoke("read_agent_repair", { sessionId }),
  cancelAgentRepair: (sessionId) => invoke("cancel_agent_repair", { sessionId }),
  applyAgentRepair: (sessionId) => invoke("apply_agent_repair", { sessionId }),
  exportAgentPatch: (sessionId) => invoke("export_agent_patch", { sessionId }),
  copyAgentTask: (repoRoot, targetId, blocker) => invoke("copy_agent_task", { repoRoot, targetId, blocker }),
  exportAgentTaskPack: (repoRoot, targetId, blocker) => invoke("export_agent_task_pack", { repoRoot, targetId, blocker }),
  previewDiagnosticReport: () => invoke("preview_diagnostic_report"),
  exportDiagnosticReport: (report) => invoke("export_diagnostic_report", { report }),
  copyDiagnosticIssueSummary: (report) => invoke("copy_diagnostic_issue_summary", { report }),
  openExternalUrl: (url) => invoke("open_external_url", { url }),
  minimize: () => invoke("window_minimize"),
  toggleMaximize: () => invoke("window_toggle_maximize"),
  close: () => invoke("window_close"),
};
