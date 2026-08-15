// SPDX-License-Identifier: MPL-2.0

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Activity, AlertTriangle, Box, Check, CheckCircle2, ChevronRight, CircleDot,
  Clock3, Copy, Download, ExternalLink, FileJson, FolderOpen, History, Languages, LoaderCircle,
    Minus, Monitor, Play, RefreshCw, RotateCcw, Settings, ShieldCheck, Square, TerminalSquare, Trash2, X,
} from "lucide-react";
import { ElasticEvidenceField } from "./ElasticEvidenceField.jsx";
import { BlockerBranch } from "./BlockerBranch.jsx";
import { PhaseEvidenceRail } from "./PhaseEvidenceRail.jsx";
import { VerificationEvidencePath } from "./VerificationEvidencePath.jsx";
import { tauriApi } from "./tauriApi.js";
import {
  VerificationWorkspaceProvider,
  useVerificationWorkspace,
} from "./VerificationWorkspaceContext.jsx";
import {
  commandText,
  derivePhaseItems,
  VISIBLE_PHASES,
  visiblePhaseForBackend,
} from "./verificationPhases.js";

const COPY = {
  zh: {
    current: "当前检查", history: "历史", settings: "设置", choose: "选择可信仓库",
    changeRepository: "更换仓库", promise: "证明这个仓库能否在本机跑通",
    intro: "真实执行，明确阻塞。没有完整机器证据，就不会显示通过。",
    noProject: "尚未选择仓库", noProjectBody: "选择来源可信但内容陌生的仓库。Verity 只检查实际清单、命令和运行结果。",
    restoring: "正在恢复上次仓库", repositoryUnavailable: "仓库无法访问", unsupportedProject: "不支持此项目",
    unavailableBody: "路径仍被保留。重新连接磁盘或网络位置后可以重试，也可以重新定位仓库。",
    retry: "重新检测", relocate: "重新定位", chooseOther: "选择其他仓库",
    inspecting: "正在检测清单与目标", target: "运行目标", phases: "本机验证路径",
    run: "确认并开始验证", running: "正在验证", cancel: "停止检查", blocked: "当前无法验证",
    verified: "机器验证完成", unverified: "已启动，但未验证", receipt: "验证凭证", export: "导出凭证",
    taskPack: "导出 Agent 任务包", firstBlocker: "首个观察到的阻塞",
    planBlocker: "计划阻断", dependencyBlocker: "依赖解析阻断", oracleBlocker: "缺少机器 Oracle", entryBlocker: "入口契约缺失", adapterBlocker: "适配器无法裁决",
    runtime: "执行环境", available: "可用", unavailable: "不可用", refresh: "重新检测",
    planBlocked: "计划被阻断", runtimeMissing: "缺少执行环境", limitedReady: "可受限运行", fullyVerifiable: "具备完整验证条件",
    agents: "Agent 修复能力", agentNotice: "只有通过非交互、目录约束、结构化补丁和取消契约的版本，才允许在应用内执行。",
    language: "语言", languageBody: "界面文案双语对齐；命令、路径和代码标识保持原文。",
    motion: "动态效果", motionBody: "动效只解释阶段变化、焦点和完成状态。",
    diagnostics: "本地诊断", diagnosticsBody: "仅生成 Verity 自身的脱敏状态与结果统计。先预览，再由你手动保存；不会自动上传。",
    previewDiagnostic: "预览诊断", exportDiagnostic: "保存 JSON", issueDiagnostic: "打开 GitHub Issue", closePreview: "关闭预览",
    runtimeStarting: "正在启动 Docker Desktop", startDocker: "启动 Docker Desktop", dockerInstall: "Docker Desktop 未安装",
    agentTest: "测试能力", agentTesting: "正在测试", agentOpen: "打开桌面端", agentCertified: "可在应用内运行", agentTaskOnly: "仅任务交接",
    agentNotInstalled: "未安装", agentDetected: "已检测", cli: "CLI", desktop: "桌面端", localService: "本地服务",
    reduced: "减少动态", full: "功能性动效", on: "开启", off: "关闭", emptyHistory: "还没有验证凭证",
    trust: "只执行来源可信的仓库。Verity 不是恶意代码沙箱。", openSource: "MPL-2.0 开源",
    internal: "当前步骤", result: "结果", signature: "本机防篡改签名",
    valid: "签名有效", invalid: "签名无效", desktopOnly: "请在 Verity 桌面端执行此操作。",
    nativeWarning: "此目标将在隔离快照中原生执行。它不是恶意代码沙箱。",
    generated: "检测时间", fingerprint: "仓库指纹", targetKind: "目标类型", command: "实际命令",
    commandSource: "命令来源", environment: "执行环境", network: "网络策略", observation: "可观察结果",
    noCommand: "此阶段没有需要执行的仓库命令。", waiting: "等待上游阶段完成。",
    planned: "已计划", completed: "已完成", failed: "已阻塞", skipped: "不适用", active: "执行中",
    step: "步骤", progress: "进度", elapsed: "耗时", estimate: "预计剩余", noEstimate: "无确定估算",
    started: "开始时间", workingDirectory: "执行位置", liveEvidence: "确定性证据（实时）",
    latestObservations: "观察（最近 3 条）", noObservations: "尚未产生观察记录。", notYetProduced: "尚未产生",
    indeterminate: "进行中", followCurrent: "返回当前阶段", logs: "日志", proof: "证据", session: "会话",
  },
  en: {
    current: "Current check", history: "History", settings: "Settings", choose: "Choose trusted repository",
    changeRepository: "Change repository", promise: "Prove whether this repository can run here",
    intro: "Real execution. A concrete blocker. No verified state without a complete machine oracle.",
    noProject: "No repository selected", noProjectBody: "Select a trusted-source repository with unfamiliar contents. Verity inspects only executable manifests, commands, and observed results.",
    restoring: "Restoring the last repository", repositoryUnavailable: "Repository cannot be accessed", unsupportedProject: "Unsupported project",
    unavailableBody: "The path is still remembered. Reconnect the drive or network location and retry, or relocate the repository.",
    retry: "Inspect again", relocate: "Relocate", chooseOther: "Choose another repository",
    inspecting: "Detecting manifests and targets", target: "Run target", phases: "Local verification path",
    run: "Confirm and start verification", running: "Verifying", cancel: "Stop check", blocked: "Verification is blocked",
    verified: "Machine verification complete", unverified: "Started, not verified", receipt: "Verification receipt", export: "Export receipt",
    taskPack: "Export Agent task pack", firstBlocker: "First observed blocker",
    planBlocker: "Plan blocker", dependencyBlocker: "Dependency resolution blocked", oracleBlocker: "Machine oracle missing", entryBlocker: "Entry contract missing", adapterBlocker: "Adapter cannot decide",
    runtime: "Execution environment", available: "Available", unavailable: "Unavailable", refresh: "Check again",
    planBlocked: "Plan is blocked", runtimeMissing: "Execution environment missing", limitedReady: "Limited run available", fullyVerifiable: "Full verification conditions available",
    agents: "Agent repair capability", agentNotice: "A version can run in-app only after non-interactive, directory confinement, structured patch, and cancellation contracts pass.",
    language: "Language", languageBody: "Interface copy is bilingual. Commands, paths, and code identifiers remain unchanged.",
    motion: "Motion", motionBody: "Motion only explains stage changes, focus, and verified completion.",
    diagnostics: "Local diagnostics", diagnosticsBody: "Generates an allowlisted report about Verity itself. Preview and save it manually; nothing is uploaded automatically.",
    previewDiagnostic: "Preview diagnostics", exportDiagnostic: "Save JSON", issueDiagnostic: "Open GitHub Issue", closePreview: "Close preview",
    runtimeStarting: "Starting Docker Desktop", startDocker: "Start Docker Desktop", dockerInstall: "Docker Desktop is not installed",
    agentTest: "Test capability", agentTesting: "Testing", agentOpen: "Open desktop app", agentCertified: "Available in app", agentTaskOnly: "Task handoff only",
    agentNotInstalled: "Not installed", agentDetected: "Detected", cli: "CLI", desktop: "Desktop", localService: "Local service",
    reduced: "Reduced motion", full: "Functional motion", on: "On", off: "Off", emptyHistory: "No verification receipts yet",
    trust: "Run trusted-source repositories only. Verity is not a malware sandbox.", openSource: "MPL-2.0 open source",
    internal: "Current step", result: "Result", signature: "Local tamper-evident signature",
    valid: "Signature valid", invalid: "Signature invalid", desktopOnly: "Use the Verity desktop app for this action.",
    nativeWarning: "This target runs natively in an isolated snapshot. It is not a malware sandbox.",
    generated: "Inspected", fingerprint: "Repository fingerprint", targetKind: "Target type", command: "Actual command",
    commandSource: "Command source", environment: "Execution environment", network: "Network policy", observation: "Observed result",
    noCommand: "This stage has no repository command to execute.", waiting: "Waiting for the preceding stage.",
    planned: "Planned", completed: "Complete", failed: "Blocked", skipped: "Not applicable", active: "Running",
    step: "Step", progress: "Progress", elapsed: "Elapsed", estimate: "Estimated remaining", noEstimate: "No deterministic estimate",
    started: "Started", workingDirectory: "Working directory", liveEvidence: "Deterministic evidence (live)",
    latestObservations: "Observations (latest 3)", noObservations: "No observations yet.", notYetProduced: "Not produced yet",
    indeterminate: "In progress", followCurrent: "Return to current phase", logs: "Logs", proof: "Evidence", session: "Session",
  },
};

const PHASE_LABELS = {
  zh: { detect: "检测", plan: "计划", acquire: "依赖", build: "构建", exercise: "测试与启动", oracle: "Oracle 验证" },
  en: { detect: "Detect", plan: "Plan", acquire: "Dependencies", build: "Build", exercise: "Test and launch", oracle: "Oracle verification" },
};

const TECHNICAL_COPY_ZH = {
  git_tracked_and_non_ignored_source: "Git \u8ddf\u8e2a\u53ca\u672a\u5ffd\u7565\u7684\u6e90\u6587\u4ef6",
  "isolated runtime": "\u9694\u79bb\u8fd0\u884c\u73af\u5883",
  "confirmed native snapshot": "\u786e\u8ba4\u540e\u7684\u672c\u673a\u9694\u79bb\u5feb\u7167",
  "confirmed-native:docker_desktop": "Docker Desktop \u786e\u8ba4\u672c\u673a\u73af\u5883",
  none: "\u65e0\u7f51\u7edc\u8bbf\u95ee",
  registry_restricted: "\u4ec5\u5141\u8bb8\u6e05\u5355\u58f0\u660e\u7684\u6ce8\u518c\u8868",
  internal_only: "\u4ec5\u9694\u79bb\u5185\u90e8\u7f51\u7edc",
  native_user_confirmed: "\u7528\u6237\u786e\u8ba4\u7684\u672c\u673a\u7f51\u7edc",
  "The composite desktop can be built and launched, but no complete machine oracle exists.": "\u53ef\u6784\u5efa\u5e76\u542f\u52a8\u684c\u9762\u5e94\u7528\uff0c\u4f46\u6ca1\u6709\u5b8c\u6574\u7684\u673a\u5668 Oracle\u3002",
  "Frontend and Rust test oracles pass before the bounded desktop launch.": "\u524d\u7aef\u4e0e Rust \u6d4b\u8bd5 Oracle \u901a\u8fc7\u540e\uff0c\u518d\u8fdb\u884c\u6709\u754c\u7684\u684c\u9762\u542f\u52a8\u3002",
  "Declared tests pass and the live application returns a non-empty HTML document without an HTTP error.": "\u58f0\u660e\u7684\u6d4b\u8bd5\u901a\u8fc7\uff0c\u4e14\u5b9e\u9645\u5e94\u7528\u8fd4\u56de\u975e\u7a7a HTML\uff0c\u6ca1\u6709 HTTP \u9519\u8bef\u3002",
  "The repository-declared test suite exits successfully.": "\u4ed3\u5e93\u58f0\u660e\u7684\u6d4b\u8bd5\u5957\u4ef6\u6210\u529f\u9000\u51fa\u3002",
  "Every Compose service must reach its declared health check.": "\u6bcf\u4e2a Compose \u670d\u52a1\u90fd\u5fc5\u987b\u901a\u8fc7\u5176\u58f0\u660e\u7684\u5065\u5eb7\u68c0\u67e5\u3002",
  "The checked-in headless test command must pass.": "\u4ed3\u5e93\u4e2d\u5df2\u63d0\u4ea4\u7684 headless \u6d4b\u8bd5\u547d\u4ee4\u5fc5\u987b\u901a\u8fc7\u3002",
  "Every declared service reached its repository health check.": "\u6240\u6709\u58f0\u660e\u7684\u670d\u52a1\u5747\u5df2\u901a\u8fc7\u4ed3\u5e93\u5065\u5eb7\u68c0\u67e5\u3002",
  "HTTP 200 returned a non-empty HTML body on the internal verification network.": "\u5185\u90e8\u9a8c\u8bc1\u7f51\u7edc\u8fd4\u56de HTTP 200 \u4e0e\u975e\u7a7a HTML \u5185\u5bb9\u3002",
  "The repository-declared machine oracle completed successfully.": "\u4ed3\u5e93\u58f0\u660e\u7684\u673a\u5668 Oracle \u5df2\u6210\u529f\u5b8c\u6210\u3002",
};

function localizeTechnical(value, locale) {
  if (!value || locale !== "zh") return value;
  return TECHNICAL_COPY_ZH[value] || value;
}

function usePreference(key, fallback) {
  const [value, setValue] = useState(() => globalThis.localStorage?.getItem(key) ?? fallback);
  const update = (next) => { setValue(next); globalThis.localStorage?.setItem(key, next); };
  return [value, update];
}

function ActionButton({ actionId, children, icon: Icon, tone = "default", loading = false, disabled = false, disabledReason = "", onClick }) {
  const unavailable = disabled || loading;
  return <button type="button" className={`vr-button vr-button--${tone}`} data-action-id={actionId} data-action-state={loading ? "pending" : disabled ? "disabled" : "ready"} aria-disabled={unavailable || undefined} aria-describedby={disabledReason ? `${actionId}-reason` : undefined} onClick={(event) => { if (unavailable) { event.preventDefault(); return; } onClick?.(event); }}>
    {loading ? <LoaderCircle className="vr-spin" aria-hidden="true" /> : Icon ? <Icon aria-hidden="true" /> : null}<span>{children}</span>
    {disabledReason ? <span className="vr-sr-only" id={`${actionId}-reason`}>{disabledReason}</span> : null}
  </button>;
}

const CLEANUP_ANALYZER_COPY = {
  zh: {
    completed: "已完成",
    not_applicable: "不适用",
    not_installed: "未安装",
    unsafe_configuration: "配置需审查",
    failed: "执行失败",
  },
  en: {
    completed: "Completed",
    not_applicable: "Not applicable",
    not_installed: "Not installed",
    unsafe_configuration: "Review configuration",
    failed: "Failed",
  },
};

const CLEANUP_ANALYZER_REASON = {
  zh: {
    knip_completed: "已按项目入口配置完成分析",
    knip_dynamic_config_report_only: "动态配置已隔离执行；结果仅供报告",
    knip_not_in_verified_dependency_graph: "已验证依赖中没有 Knip",
    cargo_machete_completed: "Rust 依赖扫描完成；结果仅供报告",
    cargo_machete_not_installed: "当前机器未安装 cargo-machete",
    vulture_completed: "Python 高置信符号扫描完成；结果仅供报告",
    vulture_not_installed: "当前 Python 环境未安装 Vulture",
    go_deadcode_completed: "当前 GOOS/GOARCH 下的不可达函数扫描完成",
    go_deadcode_not_installed: "当前机器未安装 Go deadcode",
    verified_snapshot_unavailable: "找不到与凭证对应的已验证快照",
    not_applicable_to_selected_target: "不适用于所选技术栈",
    not_a_javascript_target: "不是 JavaScript/TypeScript 目标",
    not_a_rust_target: "不是 Rust 目标",
    not_a_python_target: "不是 Python 目标",
    not_a_go_target: "不是 Go 目标",
  },
  en: {
    knip_completed: "Analyzed with the project's explicit entry configuration",
    knip_dynamic_config_report_only: "Dynamic config ran in isolation; findings are report-only",
    knip_not_in_verified_dependency_graph: "Knip is not in the verified dependency set",
    cargo_machete_completed: "Rust dependency scan completed; findings are report-only",
    cargo_machete_not_installed: "cargo-machete is not installed on this machine",
    vulture_completed: "High-confidence Python symbol scan completed; findings are report-only",
    vulture_not_installed: "Vulture is not installed in the current Python environment",
    go_deadcode_completed: "Unreachable functions were scanned for the current GOOS/GOARCH",
    go_deadcode_not_installed: "Go deadcode is not installed on this machine",
    verified_snapshot_unavailable: "The verified snapshot associated with this receipt is unavailable",
    not_applicable_to_selected_target: "Not applicable to the selected stack",
    not_a_javascript_target: "Not a JavaScript/TypeScript target",
    not_a_rust_target: "Not a Rust target",
    not_a_python_target: "Not a Python target",
    not_a_go_target: "Not a Go target",
  },
};

function CleanupDialog({ locale, receipt, candidates, analyzers, selected, session, receipts, busy, notice, onToggle, onStart, onExport, onApply, onClose }) {
  const verified = receipt?.result === "verified";
  const analyzerCopy = CLEANUP_ANALYZER_COPY[locale] || CLEANUP_ANALYZER_COPY.en;
  const analyzerReason = CLEANUP_ANALYZER_REASON[locale] || CLEANUP_ANALYZER_REASON.en;
  return <WindowPortal><div className="vr-diagnostic-overlay" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}><section className="vr-diagnostic-dialog vr-cleanup-dialog" role="dialog" aria-modal="true" aria-labelledby="cleanup-title"><header><div><Trash2 /><h2 id="cleanup-title">{locale === "zh" ? "验证后的安全清理" : "Verified cleanup"}</h2></div><button type="button" data-action-id="cleanup.close" aria-label={locale === "zh" ? "关闭" : "Close"} onClick={onClose}><X /></button></header><p>{verified ? (locale === "zh" ? "只有删除后通过相同 Oracle 的内容才会标记为可移除。" : "Only removals that pass the same oracle are marked removable.") : (locale === "zh" ? "当前结果没有完整 Oracle，因此只报告候选，不允许写回。" : "This run has no complete oracle, so candidates are report-only.")}</p><div className="vr-cleanup-analyzers" aria-label={locale === "zh" ? "分析器状态" : "Analyzer status"}>{analyzers.map((analyzer) => <div key={analyzer.analyzer} data-state={analyzer.state}><strong>{analyzer.analyzer}</strong><span>{analyzerCopy[analyzer.state] || analyzer.state}</span><small>{analyzer.finding_count} {locale === "zh" ? "项" : "findings"}</small><code>{analyzerReason[analyzer.reason_code] || (locale === "zh" ? "分析器返回未本地化错误" : "The analyzer returned an unlocalized error")}</code></div>)}</div><div className="vr-cleanup-list">{candidates.length ? candidates.map((candidate) => <label key={candidate.id} data-eligibility={candidate.eligibility}><input type="checkbox" disabled={candidate.eligibility !== "reverification_required" || Boolean(session)} checked={selected.includes(candidate.id)} onChange={() => onToggle(candidate.id)} /><span><strong>{candidate.path}</strong><small>{candidate.kind} · {candidate.size_bytes} B · {candidate.eligibility}</small><em>{candidate.evidence.join(" ")}</em></span></label>) : <p>{locale === "zh" ? "没有发现清理候选；请结合上方分析器状态判断是否完成了适用检查。" : "No cleanup candidates found. Use the analyzer status above to confirm which checks ran."}</p>}</div>{session ? <div className="vr-cleanup-status" role="status"><strong>{session.status}</strong><span>{session.verified_candidate_ids?.length || 0} / {selected.length}</span>{["planning", "analyzing", "revalidating"].includes(session.status) ? <ActionButton actionId="cleanup.cancel" icon={Square} tone="danger" onClick={() => tauriApi.cancelCleanup(session.id)}>{locale === "zh" ? "取消复验" : "Cancel revalidation"}</ActionButton> : null}</div> : null}{receipts.map((item) => <div className="vr-cleanup-receipt" key={item.id}><span>{item.removed_files.join(", ")}</span><ActionButton actionId={`cleanup.export.${item.id}`} icon={Download} loading={busy === `export:${item.id}`} onClick={() => onExport(item)}>{locale === "zh" ? "导出补丁" : "Export patch"}</ActionButton><ActionButton actionId={`cleanup.apply.${item.id}`} icon={Check} tone="primary" loading={busy === `apply:${item.id}`} onClick={() => onApply(item)}>{locale === "zh" ? "确认写回" : "Apply removal"}</ActionButton></div>)}{notice ? <p className="vr-diagnostic-notice" role="status">{notice}</p> : null}<footer><ActionButton actionId="cleanup.start" icon={ShieldCheck} tone="primary" loading={busy === "start"} disabled={!verified || !selected.length || Boolean(session)} disabledReason={!verified ? (locale === "zh" ? "需要 verified 基线" : "A verified baseline is required") : !selected.length ? (locale === "zh" ? "没有可复验候选" : "No candidates selected") : ""} onClick={onStart}>{locale === "zh" ? "删除后用相同 Oracle 复验" : "Remove and revalidate"}</ActionButton></footer></section></div></WindowPortal>;
}

function WindowPortal({ children }) {
  return createPortal(children, globalThis.document.body);
}

function StatusMark({ state }) {
  if (state === "verified" || state === "ready" || state === "done") return <CheckCircle2 className="vr-status-icon vr-status-icon--verified" aria-hidden="true" />;
  if (state === "running" || state === "planning") return <LoaderCircle className="vr-status-icon vr-status-icon--active vr-spin" aria-hidden="true" />;
  if (state === "blocked" || state === "unsupported" || state === "failed") return <AlertTriangle className="vr-status-icon vr-status-icon--blocked" aria-hidden="true" />;
  if (state === "unverified" || state === "limited") return <CircleDot className="vr-status-icon vr-status-icon--limited" aria-hidden="true" />;
  return <CircleDot className="vr-status-icon" aria-hidden="true" />;
}

function TitleBar() {
  return <header className="vr-titlebar" data-tauri-drag-region><div className="vr-brand" data-tauri-drag-region><span className="vr-shield"><ShieldCheck /></span><span>Verity</span><small>Beta</small></div><div className="vr-window-controls"><button type="button" aria-label="Minimize" data-action-id="window.minimize" onClick={() => tauriApi.minimize().catch(() => null)}><Minus /></button><button type="button" aria-label="Maximize" data-action-id="window.maximize" onClick={() => tauriApi.toggleMaximize().catch(() => null)}><Square /></button><button type="button" aria-label="Close" data-action-id="window.close" onClick={() => tauriApi.close().catch(() => null)}><X /></button></div></header>;
}

function Sidebar({ active, setActive, t }) {
  const items = [{ id: "current", label: t.current, icon: Activity }, { id: "history", label: t.history, icon: History }, { id: "settings", label: t.settings, icon: Settings }];
  return <nav className="vr-sidebar" aria-label="Primary navigation">{items.map(({ id, label, icon: Icon }) => <button key={id} type="button" data-action-id={`nav.${id}`} aria-current={active === id ? "page" : undefined} onClick={() => setActive(id)}><Icon /><span>{label}</span></button>)}<div className="vr-sidebar__foot"><ShieldCheck /><span>{t.trust}</span></div></nav>;
}

function phaseStateLabel(state, t) {
  return state === "done" ? t.completed : state === "running" ? t.active : state === "failed" ? t.failed : state === "unverified" ? t.unverified : state === "skipped" ? t.skipped : t.planned;
}

function blockerCategory(blocker, receipt, t) {
  if (receipt?.first_observed_blocker) return t.firstBlocker;
  if (/lockfile_missing|go_sum_missing/.test(blocker?.code || "")) return t.dependencyBlocker;
  if (blocker?.code === "machine_oracle_missing") return t.oracleBlocker;
  if (blocker?.code === "godot_main_scene_missing") return t.entryBlocker;
  if (blocker?.code === "python_manager_unsupported") return t.adapterBlocker;
  return t.planBlocker;
}

const BLOCKER_REASON = {
  zh: {
    node_lockfile_out_of_sync: ["依赖锁文件不同步", "package.json 与 package-lock.json 中记录的依赖不一致。"],
    lockfile_out_of_sync: ["依赖锁文件不同步", "仓库清单与锁文件不一致，确定性安装已停止。"],
    node_lockfile_missing: ["缺少 Node 依赖锁文件", "没有锁文件时无法复现同一组 Node 依赖。"],
    rust_lockfile_missing: ["缺少 Cargo.lock", "当前目标无法在未锁定依赖版本的情况下复现构建。"],
    python_lockfile_missing: ["Python 依赖未完整锁定", "依赖文件没有把所有解析结果固定到可复现版本。"],
    go_sum_missing: ["缺少 go.sum", "Go 模块依赖尚未形成可复现校验。"],
    deno_lockfile_missing: ["缺少 deno.lock", "Deno 依赖尚未锁定。"],
    machine_oracle_missing: ["缺少机器 Oracle", "目标可以启动，但没有足以签发 verified 的自动验证结果。"],
    python_manager_unsupported: ["Python 依赖方案暂不支持", "当前依赖声明无法被现有确定性适配器安全执行。"],
    godot_main_scene_missing: ["缺少 Godot 主场景", "project.godot 没有提供唯一的可启动主场景。"],
    gradle_wrapper_missing: ["Gradle Wrapper 不完整", "需要仓库提交 wrapper 脚本、properties 和 wrapper JAR，才能固定 Gradle 运行时。"],
    dotnet_lockfile_missing: ["缺少 .NET 锁文件", "每个项目都需要 packages.lock.json，才能使用 NuGet locked restore。"],
    composer_lock_missing: ["缺少 composer.lock", "PHP 依赖尚未锁定为可复现解析结果。"],
    bundler_lock_missing: ["缺少 Gemfile.lock", "Ruby 依赖尚未锁定为可复现解析结果。"],
    php_machine_oracle_missing: ["缺少 PHP 机器 Oracle", "仅验证 Composer 清单不能证明库的实际行为。"],
    ruby_machine_oracle_missing: ["缺少 Ruby 机器 Oracle", "仅解析 Gemfile 不能证明库的实际行为。"],
    deno_entry_missing: ["缺少 Deno 运行入口", "没有找到唯一、已声明的 Deno 启动任务。"],
    compose_services_missing: ["Compose 没有服务", "Compose 清单没有可执行的 services。"],
    dependency_missing: ["缺少声明依赖", "仓库命令无法解析一个已声明的依赖。"],
    system_library_missing: ["缺少系统库", "当前执行环境没有目标所需的本机系统库。"],
    planned_tool_missing: ["Verity 计划缺少工具", "生成的运行计划没有正确解析所需工具或组件。"],
    generated_command_invalid: ["Verity 生成的命令无效", "目标适配器生成了当前工具链无法接受的参数。"],
    toolchain_incompatible: ["工具链不兼容", "当前工具链版本不满足仓库声明。"],
    repository_command_failed: ["仓库命令执行失败", "实际执行的仓库命令返回了失败状态。"],
    repository_process_not_running: ["启动进程提前退出", "目标没有在受控观察窗口内保持运行。"],
    repository_changed_during_snapshot: ["仓库在快照期间发生变化", "另一个进程仍在写入源仓库；Verity 已在执行任何仓库命令前停止。"],
    runtime_unavailable: ["目标执行环境不可用", "当前机器缺少此目标所需的运行能力。"],
    target_plan_blocked: ["运行计划仍被阻断", "目标计划尚未达到可执行条件。"],
    execution_cancelled: ["验证已取消", "用户取消了当前验证会话。"],
    runner_internal_error: ["Verity 执行器内部错误", "执行器没有形成可信的项目结论。"],
  },
  en: {
    node_lockfile_out_of_sync: ["Dependency lock file is out of sync", "package.json and package-lock.json record different dependency specifications."],
    lockfile_out_of_sync: ["Dependency lock file is out of sync", "The manifest and lock file disagree, so deterministic installation stopped."],
    node_lockfile_missing: ["Node dependency lock file is missing", "A reproducible Node dependency set cannot be established without a lock file."],
    rust_lockfile_missing: ["Cargo.lock is missing", "This target cannot reproduce its dependency resolution without a lock file."],
    python_lockfile_missing: ["Python dependencies are not fully locked", "The dependency files do not pin a reproducible resolution."],
    go_sum_missing: ["go.sum is missing", "Go module dependencies do not have reproducible checksums."],
    deno_lockfile_missing: ["deno.lock is missing", "Deno dependencies are not locked."],
    machine_oracle_missing: ["Machine oracle is missing", "The target may start, but there is no automated result strong enough for verified."],
    python_manager_unsupported: ["Python dependency scheme is unsupported", "The current deterministic adapter cannot safely execute this dependency declaration."],
    godot_main_scene_missing: ["Godot main scene is missing", "project.godot does not declare one runnable main scene."],
    gradle_wrapper_missing: ["Gradle Wrapper is incomplete", "The wrapper script, properties, and wrapper JAR must be committed to pin the Gradle runtime."],
    dotnet_lockfile_missing: [".NET lock file is missing", "Every project needs packages.lock.json for NuGet locked restore."],
    composer_lock_missing: ["composer.lock is missing", "PHP dependencies are not locked to a reproducible resolution."],
    bundler_lock_missing: ["Gemfile.lock is missing", "Ruby dependencies are not locked to a reproducible resolution."],
    php_machine_oracle_missing: ["PHP machine oracle is missing", "Composer manifest validation alone cannot prove library behavior."],
    ruby_machine_oracle_missing: ["Ruby machine oracle is missing", "Resolving a Gemfile alone cannot prove library behavior."],
    deno_entry_missing: ["Deno entry is missing", "No unique declared Deno launch task was found."],
    compose_services_missing: ["Compose has no services", "The Compose manifest contains no executable services."],
    dependency_missing: ["A declared dependency is missing", "The repository command could not resolve a declared dependency."],
    system_library_missing: ["A system library is missing", "The current execution environment lacks a native library required by the target."],
    planned_tool_missing: ["Verity plan omitted a tool", "The generated plan did not resolve a required tool or component."],
    generated_command_invalid: ["Verity generated an invalid command", "The target adapter produced an argument the current toolchain cannot accept."],
    toolchain_incompatible: ["Toolchain is incompatible", "The current toolchain version does not satisfy the repository contract."],
    repository_command_failed: ["Repository command failed", "The actual repository command returned a failure status."],
    repository_process_not_running: ["Launch process exited early", "The target did not remain active during the bounded observation window."],
    repository_changed_during_snapshot: ["Repository changed during snapshot", "Another process is still writing the source repository; Verity stopped before running any repository command."],
    runtime_unavailable: ["Target runtime is unavailable", "This machine lacks a runtime capability required by the target."],
    target_plan_blocked: ["Run plan is still blocked", "The target plan has not reached executable conditions."],
    execution_cancelled: ["Verification was cancelled", "The user cancelled the current verification session."],
    runner_internal_error: ["Verity runner internal error", "The runner did not produce a trustworthy project conclusion."],
  },
};

function localizedBlocker(blocker, locale) {
  const copy = BLOCKER_REASON[locale]?.[blocker?.code];
  return {
    summary: copy?.[0] || blocker?.summary || "",
    detail: copy?.[1] || blocker?.detail || "",
  };
}

const RUNTIME_REASON = {
  zh: {
    docker_desktop_not_installed: "尚未安装 Docker Desktop。Verity 不捆绑容器运行时。",
    docker_cli_not_found: "已发现 Docker Desktop，但没有找到 Docker CLI。",
    docker_desktop_stopped: "Docker Desktop 已安装，但后台引擎尚未运行。",
    docker_engine_unreachable: "无法连接 Docker Engine。",
    docker_buildkit_unavailable: "Docker Engine 已运行，但 BuildKit 不可用。",
    docker_capability_incomplete: "Docker 已运行，但内部网络或资源能力探测未通过。",
    docker_ready: "Docker Engine、BuildKit、内部网络和资源能力均已通过。",
    docker_desktop_start_timeout: "Docker Desktop 未能在 60 秒内就绪。",
    docker_desktop_start_failed: "无法启动 Docker Desktop。",
    desktop_runtime_unavailable: "请在 Verity 桌面端检查执行环境。",
    target_runtime_not_checked: "尚未检测此目标所需的执行工具。",
    native_toolchain_ready: "此目标声明的本机工具链已就绪。",
  },
  en: {
    docker_desktop_not_installed: "Docker Desktop is not installed. Verity does not bundle a container runtime.",
    docker_cli_not_found: "Docker Desktop was found, but the Docker CLI is unavailable.",
    docker_desktop_stopped: "Docker Desktop is installed, but its engine is not running.",
    docker_engine_unreachable: "Docker Engine cannot be reached.",
    docker_buildkit_unavailable: "Docker Engine is running, but BuildKit is unavailable.",
    docker_capability_incomplete: "Docker is running, but the internal-network or resource capability probe failed.",
    docker_ready: "Docker Engine, BuildKit, internal networking, and resource capabilities passed.",
    docker_desktop_start_timeout: "Docker Desktop did not become ready within 60 seconds.",
    docker_desktop_start_failed: "Docker Desktop could not be started.",
    desktop_runtime_unavailable: "Check the execution environment in the Verity desktop app.",
    target_runtime_not_checked: "The tools required by this target have not been checked.",
    native_toolchain_ready: "The native toolchain declared by this target is ready.",
  },
};

const AGENT_REASON = {
  zh: {
    agent_not_installed: "未安装",
    agent_capability_test_required: "已安装，需测试能力",
    agent_cli_certified: "CLI 契约已通过",
    agent_desktop_detected: "桌面应用已安装",
    kimi_wrapper_contract_unsupported: "当前 Kimi 包装器不满足 Windows 自动执行契约",
    ollama_service_detected: "Ollama 服务已发现",
  },
  en: {
    agent_not_installed: "Not installed",
    agent_capability_test_required: "Installed; capability test required",
    agent_cli_certified: "CLI contract passed",
    agent_desktop_detected: "Desktop app installed",
    kimi_wrapper_contract_unsupported: "This Kimi wrapper does not meet the Windows execution contract",
    ollama_service_detected: "Ollama service detected",
  },
};

const AGENT_ACTION_ERROR = {
  zh: {
    agent_probe_timeout: "Agent 在 90 秒内没有完成能力测试。请确认网络和登录状态后重试。",
    agent_structured_probe_failed: "Agent 已响应，但没有返回 Verity 要求的结构化结果。请检查版本后重试。",
    agent_probe_start_failed: "无法启动 Agent CLI。请重新检测安装入口。",
    agent_probe_read_failed: "无法读取 Agent 能力测试结果。",
    agent_probe_wait_failed: "Agent 能力测试进程异常退出。",
    agent_directory_contract_failed: "Agent 修改了只读探针目录，未通过目录约束。",
    agent_job_attach_failed: "无法将 Agent 加入受控进程组，能力测试已阻断。",
    claude_bare_auth_required: "Claude 的安全测试使用 bare 模式，不读取桌面登录。请配置 ANTHROPIC_API_KEY 后再测试；当前仍可导出任务包。",
  },
  en: {
    agent_probe_timeout: "The Agent did not finish the capability test within 90 seconds. Check network and sign-in state, then retry.",
    agent_structured_probe_failed: "The Agent responded without the structured result Verity requires. Check the installed version and retry.",
    agent_probe_start_failed: "The Agent CLI could not be started. Detect its installation again.",
    agent_probe_read_failed: "The Agent capability result could not be read.",
    agent_probe_wait_failed: "The Agent capability process exited unexpectedly.",
    agent_directory_contract_failed: "The Agent changed the read-only probe directory and failed confinement.",
    agent_job_attach_failed: "The Agent could not be attached to a controlled process group, so the test was blocked.",
    claude_bare_auth_required: "Claude is tested in bare mode and cannot use the desktop sign-in. Configure ANTHROPIC_API_KEY to test it; task-pack handoff remains available.",
  },
};

function agentActionError(error, locale) {
  const code = String(error || "").replace(/^Error:\s*/, "").trim();
  return AGENT_ACTION_ERROR[locale]?.[code] || AGENT_ACTION_ERROR.en[code] || code;
}

const CAPABILITY_COPY = {
  zh: {
    available: "可用", unavailable: "不可用", unknown: "未知", not_checked: "尚未检测",
    not_installed: "未安装", stopped: "已停止", starting: "启动中", daemon_unreachable: "引擎不可达",
    buildkit_unavailable: "BuildKit 不可用", capability_incomplete: "能力不完整", ready: "已就绪", error: "检测错误",
  },
  en: {
    available: "Available", unavailable: "Unavailable", unknown: "Unknown", not_checked: "Not checked",
    not_installed: "Not installed", stopped: "Stopped", starting: "Starting", daemon_unreachable: "Engine unreachable",
    buildkit_unavailable: "BuildKit unavailable", capability_incomplete: "Capability incomplete", ready: "Ready", error: "Detection error",
  },
};

const RUNTIME_COMPONENT_REASON = {
  zh: {
    docker_cli_ready: "Docker CLI 可用", docker_cli_not_found: "未找到 Docker CLI",
    docker_engine_ready: "Docker Engine 可用", docker_engine_unreachable: "Docker Engine 未启动或无法连接", docker_engine_not_checked: "尚未检测 Engine",
    docker_buildkit_ready: "BuildKit 构建探测通过", docker_buildkit_unavailable: "BuildKit 构建探测失败", docker_buildkit_not_checked: "待 Engine 启动后检测",
    docker_network_ready: "隔离网络探测通过", docker_network_probe_failed: "隔离网络探测失败", docker_network_not_checked: "待 Engine 启动后检测",
    docker_limits_ready: "资源限制能力可用", docker_limits_probe_failed: "资源限制探测失败", docker_limits_not_checked: "待 Engine 启动后检测",
  },
  en: {
    docker_cli_ready: "Docker CLI is available", docker_cli_not_found: "Docker CLI was not found",
    docker_engine_ready: "Docker Engine is available", docker_engine_unreachable: "Docker Engine is stopped or unreachable", docker_engine_not_checked: "Engine has not been checked",
    docker_buildkit_ready: "BuildKit build probe passed", docker_buildkit_unavailable: "BuildKit build probe failed", docker_buildkit_not_checked: "Checked after Engine starts",
    docker_network_ready: "Isolated network probe passed", docker_network_probe_failed: "Isolated network probe failed", docker_network_not_checked: "Checked after Engine starts",
    docker_limits_ready: "Resource limits are available", docker_limits_probe_failed: "Resource limit probe failed", docker_limits_not_checked: "Checked after Engine starts",
  },
};

function runtimeReason(runtime, locale) {
  return RUNTIME_REASON[locale]?.[runtime?.reason_code] || runtime?.reason_code || RUNTIME_REASON[locale]?.desktop_runtime_unavailable;
}

function targetRuntimeReason(target, locale) {
  const reason = target?.environment_reason_code || "target_runtime_not_checked";
  if (reason.startsWith("native_toolchain_missing:")) {
    const tools = reason.slice("native_toolchain_missing:".length);
    return locale === "zh" ? `缺少此目标需要的本机工具：${tools}` : `Native tools required by this target are missing: ${tools}`;
  }
  return RUNTIME_REASON[locale]?.[reason] || reason;
}

function capabilityLabel(installation, t) {
  if (installation.status === "not_installed") return t.agentNotInstalled;
  if (installation.status === "certified") return t.agentCertified;
  return t.agentDetected;
}

function CurrentCheckExperience({ locale, t, runtime, motion }) {
  const {
    repositoryRoot, plan, selectedId, target, session, receipt, busy, error, restoreState,
    chooseRepository, retryRestore, selectTarget, run, cancel, exportReceipt, exportTaskPack,
  } = useVerificationWorkspace();
  const [focusedPhase, setFocusedPhase] = useState("plan");
  const [following, setFollowing] = useState(true);
  const [evidenceMode, setEvidenceMode] = useState("summary");
  const [agentCapabilities, setAgentCapabilities] = useState([]);
  const [agentProvider, setAgentProvider] = useState("");
  const [agentRepair, setAgentRepair] = useState(null);
  const [agentBusy, setAgentBusy] = useState("");
  const [agentPatchOpen, setAgentPatchOpen] = useState(false);
  const [agentNotice, setAgentNotice] = useState("");
  const [cleanupOpen, setCleanupOpen] = useState(false);
  const [cleanupCandidates, setCleanupCandidates] = useState([]);
  const [cleanupAnalyzers, setCleanupAnalyzers] = useState([]);
  const [cleanupSelected, setCleanupSelected] = useState([]);
  const [cleanupSession, setCleanupSession] = useState(null);
  const [cleanupReceipts, setCleanupReceipts] = useState([]);
  const [cleanupBusy, setCleanupBusy] = useState("");
  const [cleanupNotice, setCleanupNotice] = useState("");
  const evidenceFieldRef = useRef(null);
  const activePhase = receipt?.result === "verified" ? "oracle" : visiblePhaseForBackend(session?.current_phase);

  useEffect(() => {
    if (following && activePhase) setFocusedPhase(activePhase);
  }, [activePhase, following]);

  const sessionBlocker = session?.failure_code ? {
    phase: session.current_phase || "detect",
    origin: session.failure_origin,
    code: session.failure_code,
    summary: session.message,
    detail: session.error,
  } : null;
  const blocker = receipt?.first_observed_blocker || sessionBlocker || target?.blockers?.[0];
  useEffect(() => {
    if (!blocker || !tauriApi.available()) return;
    void tauriApi.listAgentCapabilities().then((items) => {
      setAgentCapabilities(items);
      const certified = items.find((item) => item.usable_in_app);
      setAgentProvider((current) => current || certified?.provider || "");
    }).catch(() => setAgentCapabilities([]));
  }, [blocker]);
  useEffect(() => {
    if (!agentRepair || !["pending", "running"].includes(agentRepair.status)) return;
    const timer = globalThis.setInterval(async () => {
      const next = await tauriApi.readAgentRepair(agentRepair.id).catch(() => null);
      if (next) setAgentRepair(next);
    }, 750);
    return () => globalThis.clearInterval(timer);
  }, [agentRepair?.id, agentRepair?.status]);
  useEffect(() => {
    if (!cleanupSession || !["planning", "analyzing", "revalidating"].includes(cleanupSession.status)) return;
    const timer = globalThis.setInterval(async () => {
      const next = await tauriApi.readCleanupSession(cleanupSession.id).catch(() => null);
      if (!next) return;
      setCleanupSession(next);
      if (["completed", "blocked", "cancelled", "internal_error"].includes(next.status)) {
        setCleanupReceipts(await tauriApi.listCleanupReceipts(next.id).catch(() => []));
      }
    }, 1000);
    return () => globalThis.clearInterval(timer);
  }, [cleanupSession?.id, cleanupSession?.status]);
  const blockerPhase = blocker?.phase ? visiblePhaseForBackend(blocker.phase) : null;
  const blockerText = localizedBlocker(blocker, locale);
  const native = target?.commands?.some((command) => command.native);
  const runtimeReady = native ? target?.environment_status === "ready" : runtime?.status === "ready";
  const canRun = target?.plan_status === "complete" && runtimeReady;
  const limited = target?.plan_status === "complete" && target?.oracle_status !== "machine";
  const blocked = Boolean(blocker || receipt?.result === "blocked" || session?.status === "blocked" || session?.status === "internal_error");
  const running = busy === "run" || session?.status === "running";
  const startedUnverified = receipt?.result === "started_unverified" || session?.status === "started_unverified";
  const fieldStatus = receipt?.result === "verified" ? "verified" : startedUnverified ? "unverified" : blocked ? "blocked" : running ? "running" : "idle";
  const fieldPhase = blocked && blockerPhase ? blockerPhase : activePhase;
  const runDisabledReason = !target ? t.blocked : target.plan_status !== "complete" ? (localizedBlocker(target.blockers?.find((item) => item.origin !== "oracle"), locale).summary || t.blocked) : !runtimeReady ? (native ? targetRuntimeReason(target, locale) : runtimeReason(runtime, locale)) : "";
  const eligibility = !target ? { state: "blocked", label: t.planBlocked }
    : target.plan_status === "unsupported" ? { state: "unsupported", label: t.unsupportedProject }
      : target.plan_status !== "complete" ? { state: "blocked", label: t.planBlocked }
        : !runtimeReady ? { state: "blocked", label: t.runtimeMissing }
          : limited ? { state: "limited", label: t.limitedReady }
            : { state: "ready", label: t.fullyVerifiable };
  const contextStatus = running ? { state: "running", label: t.running }
    : fieldStatus === "verified" ? { state: "verified", label: t.verified }
      : fieldStatus === "unverified" ? { state: "unverified", label: t.unverified }
        : receipt || session?.status === "blocked" || session?.status === "internal_error" ? { state: "blocked", label: t.blocked }
          : eligibility;
  const productTargets = plan?.targets?.filter((item) => ["product", "service"].includes(item.role)) || [];
  const advancedTargets = plan?.targets?.filter((item) => !["product", "service"].includes(item.role)) || [];

  useEffect(() => {
    if (blockerPhase && !session && !receipt) {
      setFocusedPhase(blockerPhase);
      setFollowing(false);
    }
  }, [blockerPhase, receipt, session]);
  const phaseItems = useMemo(() => derivePhaseItems(plan, target, session, receipt).map((item) => {
    const command = commandText(item.commands[0]);
    const detail = item.id === activePhase && session?.message ? session.message
      : item.state === "failed" ? (blockerText.summary || t.failed)
        : item.id === "detect" ? localizeTechnical(plan?.source_scope, locale)
          : item.id === "plan" ? localizeTechnical(target?.oracle?.description, locale)
            : item.id === "oracle" ? localizeTechnical(receipt?.oracle?.detail || target?.oracle?.description, locale)
              : command || (item.state === "skipped" ? t.noCommand : t.waiting);
    return { ...item, detail };
  }), [activePhase, blockerText.summary, locale, plan, receipt, session, t.failed, t.noCommand, t.waiting, target]);
  const focusedItem = phaseItems.find((item) => item.id === focusedPhase) || phaseItems[0];
  const labels = {
    ...t,
    phases: t.phases,
    phaseNames: PHASE_LABELS[locale],
    phaseOrder: Object.fromEntries(VISIBLE_PHASES.map((phase, index) => [phase.id, index + 1])),
    phaseState: (state) => phaseStateLabel(state, t),
    formatTechnical: (value) => localizeTechnical(value, locale),
  };

  function focusPhase(phase) {
    setFocusedPhase(phase);
    setFollowing(phase === activePhase);
    setEvidenceMode("summary");
  }

  function activatePhaseVisual(phase, anchorElement) {
    evidenceFieldRef.current?.activateSelection(phase, anchorElement);
  }

  function followCurrent() {
    setFollowing(true);
    setFocusedPhase(activePhase || "plan");
    setEvidenceMode("summary");
  }

  function focusBlocker() {
    if (!blockerPhase) return;
    const control = document.querySelector(`[data-phase-id="${blockerPhase}"]`);
    activatePhaseVisual(blockerPhase, control);
    focusPhase(blockerPhase);
    control?.focus();
  }

  async function copyTaskPack() {
    setAgentBusy("copy");
    setAgentNotice("");
    try {
      await tauriApi.copyAgentTask(repositoryRoot, selectedId, blocker || null);
      setAgentNotice(locale === "zh" ? "Agent 任务已复制" : "Agent task copied");
    } catch (nextError) { setAgentNotice(String(nextError)); }
    finally { setAgentBusy(""); }
  }

  async function startAgentRepair() {
    if (!agentProvider) return;
    setAgentBusy("repair");
    try { setAgentRepair(await tauriApi.startAgentRepair(repositoryRoot, selectedId, agentProvider)); }
    finally { setAgentBusy(""); }
  }

  async function cancelAgentRepair() {
    if (!agentRepair) return;
    setAgentRepair(await tauriApi.cancelAgentRepair(agentRepair.id));
  }

  async function exportAgentPatch() {
    if (!agentRepair) return;
    setAgentBusy("export-patch");
    setAgentNotice("");
    try {
      const path = await tauriApi.exportAgentPatch(agentRepair.id);
      if (path) setAgentNotice(locale === "zh" ? `补丁已保存：${path}` : `Patch saved: ${path}`);
    } catch (nextError) { setAgentNotice(String(nextError)); }
    finally { setAgentBusy(""); }
  }

  async function applyAgentPatch() {
    if (!agentRepair) return;
    const confirmed = globalThis.confirm(locale === "zh" ? "仅当原文件哈希未变化时才会写回。确认应用这份已复验补丁？" : "The patch is written only if every original file hash is unchanged. Apply this verified patch?");
    if (!confirmed) return;
    setAgentBusy("apply-patch");
    setAgentNotice("");
    try {
      await tauriApi.applyAgentRepair(agentRepair.id);
      setAgentNotice(locale === "zh" ? "补丁已写回原仓库。" : "Patch applied to the original repository.");
    } catch (nextError) { setAgentNotice(String(nextError)); }
    finally { setAgentBusy(""); }
  }

  async function openCleanup() {
    if (!receipt) return;
    setCleanupBusy("preview");
    setCleanupNotice("");
    try {
      const preview = await tauriApi.previewCleanup(repositoryRoot, receipt.id);
      setCleanupCandidates(preview.candidates);
      setCleanupAnalyzers(preview.analyzers);
      setCleanupSelected(preview.candidates.filter((item) => item.eligibility === "reverification_required").map((item) => item.id));
      setCleanupSession(null);
      setCleanupReceipts([]);
      setCleanupOpen(true);
    } catch (nextError) {
      setCleanupCandidates([]);
      setCleanupAnalyzers([]);
      setCleanupNotice(String(nextError));
      setCleanupOpen(true);
    } finally { setCleanupBusy(""); }
  }

  async function startCleanup() {
    if (receipt?.result !== "verified" || !cleanupSelected.length) return;
    setCleanupBusy("start");
    setCleanupNotice("");
    try { setCleanupSession(await tauriApi.startCleanup(repositoryRoot, receipt.id, cleanupSelected)); }
    catch (nextError) { setCleanupNotice(String(nextError)); }
    finally { setCleanupBusy(""); }
  }

  async function exportCleanup(item) {
    setCleanupBusy(`export:${item.id}`);
    setCleanupNotice("");
    try {
      const path = await tauriApi.exportCleanupPatch(cleanupSession.id, item.id);
      setCleanupNotice(path ? (locale === "zh" ? `补丁已保存：${path}` : `Patch saved: ${path}`) : (locale === "zh" ? "已取消导出。" : "Export cancelled."));
    } catch (nextError) { setCleanupNotice(String(nextError)); }
    finally { setCleanupBusy(""); }
  }

  async function applyCleanup(item) {
    const confirmed = globalThis.confirm(locale === "zh" ? "仅在原文件哈希未变化时删除。确认写回？" : "Removal is allowed only if source hashes are unchanged. Apply?");
    if (!confirmed) return;
    setCleanupBusy(`apply:${item.id}`);
    setCleanupNotice("");
    try {
      await tauriApi.applyCleanup(repositoryRoot, cleanupSession.id, item.id);
      setCleanupNotice(locale === "zh" ? "已按复验凭证写回；请重新检查仓库。" : "The verified removal was applied. Inspect the repository again.");
    } catch (nextError) { setCleanupNotice(String(nextError)); }
    finally { setCleanupBusy(""); }
  }

  function toggleCleanupCandidate(id) {
    setCleanupSelected((items) => items.includes(id) ? items.filter((item) => item !== id) : [...items, id]);
  }

  if (!plan) {
    const unavailable = restoreState === "unavailable" && repositoryRoot;
    return <div className="vr-page vr-current-page"><section className="vr-empty">
      <div className="vr-empty-mark" aria-hidden="true"><ShieldCheck /></div>
      <h1>{busy === "inspect" ? t.restoring : unavailable && /no supported project manifest/i.test(error) ? t.unsupportedProject : unavailable ? t.repositoryUnavailable : t.noProject}</h1>
      <p>{busy === "inspect" ? repositoryRoot : unavailable ? t.unavailableBody : t.noProjectBody}</p>
      {error && busy !== "inspect" ? <div className="vr-inline-error" role="alert"><AlertTriangle /><span>{error}</span></div> : null}
      <div className="vr-empty-actions">
        {unavailable ? <ActionButton actionId="repository.restore" icon={RotateCcw} loading={busy === "inspect"} onClick={retryRestore}>{t.retry}</ActionButton> : null}
        <ActionButton actionId={unavailable ? "repository.relocate" : "repository.choose"} icon={FolderOpen} tone="primary" loading={busy === "inspect"} onClick={chooseRepository}>{unavailable ? t.relocate : t.choose}</ActionButton>
        {unavailable ? <ActionButton actionId="repository.choose-other" icon={FolderOpen} onClick={chooseRepository}>{t.chooseOther}</ActionButton> : null}
      </div>
    </section></div>;
  }

  return <div className="vr-page vr-current-page vr-current-experience">
    <header className="vr-repository-context">
      <div className="vr-repository-context__identity"><FolderOpen aria-hidden="true" /><span><strong>{plan.repository_name}</strong><code title={repositoryRoot}>{repositoryRoot}</code></span></div>
      <label className="vr-target-select"><span>{t.target}</span><select data-action-id="target.select" value={selectedId} disabled={running} onChange={(event) => selectTarget(event.target.value)}><option value="" disabled>{locale === "zh" ? "请选择产品目标" : "Choose a product target"}</option><optgroup label={locale === "zh" ? "产品目标" : "Product targets"}>{productTargets.map((item) => <option key={item.id} value={item.id}>{item.recommended ? "● " : ""}{item.label} · {item.stack}/{item.kind}</option>)}</optgroup>{advancedTargets.length ? <optgroup label={locale === "zh" ? "高级组件（不代表完整产品）" : "Advanced components (not the full product)"}>{advancedTargets.map((item) => <option key={item.id} value={item.id}>{item.label} · {item.role} · {item.relative_root || "."}</option>)}</optgroup> : null}</select></label>
      <div className="vr-context-status" data-state={contextStatus.state}><StatusMark state={contextStatus.state} /><span>{contextStatus.label}</span></div>
      <ActionButton actionId="repository.choose" icon={FolderOpen} disabled={running} disabledReason={running ? t.running : ""} loading={busy === "inspect"} onClick={chooseRepository}>{t.changeRepository}</ActionButton>
    </header>
    {error ? <div className="vr-current-error" role="alert"><AlertTriangle /><span>{error}</span></div> : null}
    <section className="vr-verification-stage" aria-label={t.phases}>
      <div className="vr-verification-stage__path">
        <ElasticEvidenceField ref={evidenceFieldRef} phase={fieldPhase} status={fieldStatus} heartbeat={session?.progress?.heartbeat_at || session?.updated_at} motionProfile={motion} paused={false} />
        <VerificationEvidencePath items={phaseItems} labels={labels} focusedPhase={focusedPhase} currentPhase={activePhase} onFocusPhase={focusPhase} onVisualActivate={activatePhaseVisual} />
        {blocker && blockerPhase ? <BlockerBranch phase={blockerPhase} label={blockerCategory(blocker, receipt, t)} summary={blockerText.summary} onActivate={focusBlocker} /> : null}
      </div>
      <PhaseEvidenceRail item={focusedItem} phaseName={PHASE_LABELS[locale][focusedItem.id]} currentPhase={activePhase} following={following} onFollowCurrent={followCurrent} mode={evidenceMode} plan={plan} target={target} repositoryRoot={repositoryRoot} session={session} receipt={receipt} blockerText={blockerText} labels={labels} />
    </section>
    <footer className="vr-session-bar">
      <div className="vr-session-id"><span>{t.session}</span><code>{session?.id || t.notYetProduced}</code></div>
      <div className="vr-session-views">
        <button type="button" data-action-id="evidence.focus.logs" aria-pressed={evidenceMode === "logs"} onClick={() => setEvidenceMode("logs")}>{t.logs}</button>
        <button type="button" data-action-id="evidence.focus.receipt" aria-pressed={evidenceMode === "receipt"} aria-disabled={!receipt || undefined} title={!receipt ? t.notYetProduced : ""} onClick={() => receipt && setEvidenceMode("receipt")}>{t.proof}</button>
      </div>
      <div className="vr-session-actions">
        {blocker && !running ? <ActionButton actionId="agent.copy-task" icon={Copy} loading={agentBusy === "copy"} onClick={copyTaskPack}>{locale === "zh" ? "复制 Agent 任务" : "Copy Agent task"}</ActionButton> : null}
        {blocker && !running ? <ActionButton actionId="agent.export-task-pack" icon={Download} onClick={exportTaskPack}>{t.taskPack}</ActionButton> : null}
        {blocker && !running && agentCapabilities.some((item) => item.usable_in_app) ? <label className="vr-agent-run-select"><select data-action-id="agent.provider.select" value={agentProvider} onChange={(event) => setAgentProvider(event.target.value)}>{agentCapabilities.filter((item) => item.usable_in_app).map((item) => <option key={item.provider} value={item.provider}>{item.provider}</option>)}</select>{agentRepair?.status === "running" ? <ActionButton actionId="agent.repair.cancel" icon={Square} tone="danger" onClick={cancelAgentRepair}>{locale === "zh" ? "取消 Agent" : "Cancel Agent"}</ActionButton> : <ActionButton actionId="agent.repair.start" icon={TerminalSquare} loading={agentBusy === "repair"} onClick={startAgentRepair}>{locale === "zh" ? "受限修复" : "Restricted repair"}</ActionButton>}</label> : null}
        {agentRepair && !["pending", "running"].includes(agentRepair.status) ? <span className="vr-agent-repair-result" data-state={agentRepair.status}>{agentRepair.status}{agentRepair.verification_result ? ` · ${agentRepair.verification_result}` : ""}{agentRepair.error_code ? ` · ${agentRepair.error_code}` : ""}</span> : null}
        {agentNotice ? <span className="vr-agent-repair-result" role="status">{agentNotice}</span> : null}
        {agentRepair?.status === "completed" && agentRepair.output ? <ActionButton actionId="agent.repair.review" icon={FileJson} onClick={() => setAgentPatchOpen(true)}>{locale === "zh" ? "审阅补丁" : "Review patch"}</ActionButton> : null}
        {receipt ? <ActionButton actionId="cleanup.preview" icon={Trash2} loading={cleanupBusy === "preview"} onClick={openCleanup}>{locale === "zh" ? "检查可移除内容" : "Check removable content"}</ActionButton> : null}
        {receipt ? <ActionButton actionId="receipt.export" icon={Download} onClick={exportReceipt}>{t.export}</ActionButton> : null}
        {running ? <ActionButton actionId="run.cancel" icon={Square} tone="danger" onClick={cancel}>{t.cancel}</ActionButton> : <ActionButton actionId="run.start" icon={Play} tone="primary" disabled={!canRun} disabledReason={runDisabledReason} onClick={run}>{t.run}</ActionButton>}
      </div>
    </footer>
    {agentPatchOpen && agentRepair?.output ? <WindowPortal><div className="vr-diagnostic-overlay" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setAgentPatchOpen(false); }}><section className="vr-diagnostic-dialog vr-agent-patch-dialog" role="dialog" aria-modal="true" aria-labelledby="agent-patch-title"><header><div><FileJson /><h2 id="agent-patch-title">{locale === "zh" ? "已复验 Agent 补丁" : "Verified Agent patch"}</h2></div><button type="button" data-action-id="agent.repair.review.close" aria-label={locale === "zh" ? "关闭" : "Close"} onClick={() => setAgentPatchOpen(false)}><X /></button></header><div className="vr-agent-patch-evidence"><span>{agentRepair.verification_result}</span><p>{agentRepair.output.evidence}</p></div><pre>{agentRepair.output.unified_diff}</pre>{agentNotice ? <p className="vr-diagnostic-notice">{agentNotice}</p> : null}<footer><ActionButton actionId="agent.repair.patch.export" icon={Download} loading={agentBusy === "export-patch"} onClick={exportAgentPatch}>{locale === "zh" ? "导出补丁" : "Export patch"}</ActionButton><ActionButton actionId="agent.repair.patch.apply" icon={Check} tone="primary" loading={agentBusy === "apply-patch"} onClick={applyAgentPatch}>{locale === "zh" ? "写回仓库" : "Apply to repository"}</ActionButton></footer></section></div></WindowPortal> : null}
    {cleanupOpen ? <CleanupDialog locale={locale} receipt={receipt} candidates={cleanupCandidates} analyzers={cleanupAnalyzers} selected={cleanupSelected} session={cleanupSession} receipts={cleanupReceipts} busy={cleanupBusy} notice={cleanupNotice} onToggle={toggleCleanupCandidate} onStart={startCleanup} onExport={exportCleanup} onApply={applyCleanup} onClose={() => setCleanupOpen(false)} /> : null}
  </div>;
}

function HistoryPage({ t, version, locale }) {
  const [items, setItems] = useState([]); const [selected, setSelected] = useState(null); const [signature, setSignature] = useState(null); const [loadError, setLoadError] = useState("");
  useEffect(() => {
    let disposed = false;
    if (!tauriApi.available()) {
      setLoadError(locale === "zh" ? "\u5386\u53f2\u51ed\u8bc1\u4ec5\u5728\u684c\u9762\u7aef\u53ef\u7528\u3002" : "Receipt history is only available in the desktop app.");
      return () => { disposed = true; };
    }
    setLoadError("");
    tauriApi.listReceipts().then((nextItems) => {
      if (!disposed) setItems(nextItems);
    }).catch((error) => {
      if (!disposed) { setItems([]); setLoadError(String(error)); }
    });
    return () => { disposed = true; };
  }, [version, locale]);
  async function select(item) {
    setSelected(item); setSignature(null);
    try { setSignature(await tauriApi.verifyReceipt(item.id)); }
    catch (error) { setLoadError(String(error)); }
  }
  const groups = useMemo(() => {
    const grouped = new Map();
    for (const item of items) {
      const key = `${item.repository_name}\0${item.target_id}`;
      const group = grouped.get(key) || { latest: item, count: 0 };
      group.count += 1;
      if (item.created_at > group.latest.created_at) group.latest = item;
      grouped.set(key, group);
    }
    return [...grouped.values()].sort((a, b) => b.latest.created_at.localeCompare(a.latest.created_at));
  }, [items]);
  const copy = locale === "zh" ? {
    summary: `${groups.length} \u4e2a\u4ea7\u54c1\u76ee\u6807 \u00b7 ${items.length} \u4efd\u51ed\u8bc1`, runs: "\u6b21\u8fd0\u884c", blocker: "\u963b\u585e\u6765\u6e90", host: "\u4e3b\u673a", environment: "\u6267\u884c\u73af\u5883",
  } : {
    summary: `${groups.length} product target${groups.length === 1 ? "" : "s"} \u00b7 ${items.length} receipt${items.length === 1 ? "" : "s"}`, runs: "runs", blocker: "Blocker source", host: "Host", environment: "Environment",
  };
  const resultLabel = (value) => locale === "zh" ? ({ verified: "\u5df2\u9a8c\u8bc1", started_unverified: "\u5df2\u542f\u52a8\u672a\u9a8c\u8bc1", blocked: "\u5df2\u963b\u585e" }[value] || value) : value;
  const originLabel = (value) => locale === "zh" ? ({ repository: "\u4ed3\u5e93", verity_plan: "Verity \u8ba1\u5212", runtime: "\u6267\u884c\u73af\u5883", oracle: "Oracle", user: "\u7528\u6237" }[value] || value) : value;
  return <div className="vr-page"><header className="vr-page-header"><div><h1>{t.history}</h1><span>{copy.summary}</span></div></header>{loadError ? <div className="vr-current-error" role="alert"><AlertTriangle /><span>{loadError}</span></div> : null}{!loadError && !items.length ? <section className="vr-empty vr-empty--small"><Clock3 /><h2>{t.emptyHistory}</h2></section> : null}{items.length ? <section className="vr-history"><div className="vr-history-list">{groups.map(({ latest: item, count }) => <button type="button" key={`${item.repository_name}.${item.target_id}`} data-action-id={`history.open.${item.id}`} aria-pressed={selected?.id === item.id} onClick={() => select(item)}><StatusMark state={item.result === "verified" ? "verified" : item.result === "started_unverified" ? "unverified" : "blocked"} /><span><strong>{item.repository_name}{" · "}{item.target_label || item.target_id}</strong><small>{new Date(item.created_at).toLocaleString()}{" · "}{item.stack}/{item.kind}{" · "}{count} {copy.runs}</small></span><code>{item.first_observed_blocker ? originLabel(item.first_observed_blocker.origin) : resultLabel(item.result)}</code><ChevronRight /></button>)}</div>{selected ? <aside className="vr-history-detail"><h2>{selected.repository_name}{" · "}{selected.target_label || selected.target_id}</h2><p>{localizeTechnical(selected.oracle.detail, locale)}</p><dl><div><dt>{t.result}</dt><dd>{resultLabel(selected.result)}</dd></div>{selected.first_observed_blocker ? <div><dt>{copy.blocker}</dt><dd>{originLabel(selected.first_observed_blocker.origin)}</dd></div> : null}<div><dt>{t.signature}</dt><dd className={signature === true ? "is-valid" : signature === false ? "is-invalid" : ""}>{signature === null ? "…" : signature ? t.valid : t.invalid}</dd></div><div><dt>{copy.host}</dt><dd>{selected.host_os}/{selected.host_arch}</dd></div><div><dt>{copy.environment}</dt><dd>{localizeTechnical(selected.execution_environment, locale)}</dd></div></dl><ActionButton actionId="history.export" icon={Download} onClick={() => tauriApi.exportReceipt(selected.id)}>{t.export}</ActionButton></aside> : null}</section> : null}</div>;
}

function SettingsPage({ locale, setLocale, motion, setMotion, runtime, refreshRuntime, t }) {
  const [agents, setAgents] = useState([]);
  const [pending, setPending] = useState("");
  const [error, setError] = useState("");
  const [diagnostic, setDiagnostic] = useState(null);
  const [diagnosticNotice, setDiagnosticNotice] = useState("");
  async function refreshAgents() {
    setPending("agents.refresh"); setError("");
    try { setAgents(await tauriApi.listAgentCapabilities()); } catch (nextError) { setError(String(nextError)); }
    finally { setPending(""); }
  }
  useEffect(() => { if (tauriApi.available()) void refreshAgents(); }, []);
  useEffect(() => { setError(""); }, [locale]);
  async function startDocker() {
    setPending("runtime.start"); setError("");
    try { await tauriApi.startDockerDesktop(); await refreshRuntime(); } catch (nextError) { setError(String(nextError)); await refreshRuntime(); }
    finally { setPending(""); }
  }
  async function testAgent(provider) {
    setPending(`agent.test.${provider}`); setError("");
    try {
      const updated = await tauriApi.testAgentCapability(provider);
      setAgents((items) => items.map((item) => item.provider === provider ? updated : item));
    } catch (nextError) { setError(agentActionError(nextError, locale)); }
    finally { setPending(""); }
  }
  async function previewDiagnostic() {
    setPending("diagnostic.preview"); setError(""); setDiagnosticNotice("");
    try { setDiagnostic(await tauriApi.previewDiagnosticReport()); } catch (nextError) { setError(String(nextError)); }
    finally { setPending(""); }
  }
  async function exportDiagnostic() {
    if (!diagnostic) return;
    setPending("diagnostic.export");
    try { const path = await tauriApi.exportDiagnosticReport(diagnostic); if (path) setDiagnosticNotice(path); } catch (nextError) { setError(String(nextError)); }
    finally { setPending(""); }
  }
  async function copyIssueSummary() {
    if (!diagnostic) return;
    await tauriApi.copyDiagnosticIssueSummary(diagnostic);
    setDiagnosticNotice(locale === "zh" ? "Issue 摘要已复制" : "Issue summary copied");
  }
  function openIssue() {
    if (!diagnostic) return;
    const body = `Verity ${diagnostic.app_version}%0AHost: ${diagnostic.host_os}/${diagnostic.host_arch}%0ARuntime: ${diagnostic.runtime_status} (${diagnostic.runtime_reason_code})%0A%0APlease attach the local diagnostic JSON only after reviewing it.`;
    void tauriApi.openExternalUrl(`https://github.com/logi-cmd/verity/issues/new?title=Verity%20diagnostic&body=${body}`);
  }
  const runtimeChecks = runtime ? [
    ["CLI", runtime.cli], ["Engine", runtime.engine], ["BuildKit", runtime.buildkit],
    ["Internal network", runtime.internal_network], ["Resource limits", runtime.resource_limits],
  ] : [];
  const canStartDocker = runtime?.launchable && ["stopped", "daemon_unreachable"].includes(runtime?.status);
  const capabilityCopy = CAPABILITY_COPY[locale] || CAPABILITY_COPY.en;
  const componentReason = RUNTIME_COMPONENT_REASON[locale] || RUNTIME_COMPONENT_REASON.en;
  return <div className="vr-page"><header className="vr-page-header"><div><h1>{t.settings}</h1><span>{t.openSource}</span></div></header>
    {error ? <div className="vr-current-error" role="alert"><AlertTriangle /><span>{error}</span></div> : null}
    <section className="vr-settings-section"><header><Languages /><div><h2>{t.language}</h2><p>{t.languageBody}</p></div></header><div className="vr-segment"><button type="button" data-action-id="settings.locale.zh" aria-pressed={locale === "zh"} onClick={() => setLocale("zh")}>中文</button><button type="button" data-action-id="settings.locale.en" aria-pressed={locale === "en"} onClick={() => setLocale("en")}>English</button></div></section>
    <section className="vr-settings-section"><header><Activity /><div><h2>{t.motion}</h2><p>{t.motionBody}</p></div></header><div className="vr-segment"><button type="button" data-action-id="settings.motion.full" aria-pressed={motion === "full"} onClick={() => setMotion("full")}>{t.full}</button><button type="button" data-action-id="settings.motion.reduced" aria-pressed={motion === "reduced"} onClick={() => setMotion("reduced")}>{t.reduced}</button></div></section>
    <section className="vr-settings-section vr-settings-section--stacked"><header><Box /><div><h2>{t.runtime}</h2><p>{runtimeReason(runtime, locale)}</p></div></header><div className="vr-runtime-detail"><div className="vr-runtime-summary" data-state={runtime?.status}><StatusMark state={runtime?.status === "ready" ? "ready" : runtime?.status === "starting" ? "running" : "blocked"} /><strong>{capabilityCopy[runtime?.status] || t.unavailable}</strong><div className="vr-settings-action">{canStartDocker ? <ActionButton actionId="settings.runtime.start" icon={Play} loading={pending === "runtime.start"} onClick={startDocker}>{pending === "runtime.start" ? t.runtimeStarting : t.startDocker}</ActionButton> : null}<ActionButton actionId="settings.runtime.refresh" icon={RefreshCw} loading={pending === "runtime.refresh"} onClick={async () => { setPending("runtime.refresh"); await refreshRuntime(); setPending(""); }}>{t.refresh}</ActionButton></div></div><div className="vr-capability-grid">{runtimeChecks.map(([label, value]) => <div key={label}><span>{label}</span><strong data-state={value?.state}>{capabilityCopy[value?.state] || capabilityCopy.not_checked}</strong><code>{value?.version || componentReason[value?.reason_code] || value?.reason_code}</code></div>)}</div></div></section>
    <section className="vr-settings-section vr-settings-section--stacked"><header><TerminalSquare /><div><h2>{t.agents}</h2><p>{t.agentNotice}</p></div></header><div className="vr-agent-list">{agents.map((agent) => <section key={agent.provider} className="vr-agent-provider"><header><StatusMark state={agent.usable_in_app ? "ready" : "limited"} /><strong>{agent.provider}</strong><span>{agent.usable_in_app ? t.agentCertified : t.agentTaskOnly}</span>{agent.installations.some((item) => item.channel === "cli" && item.status === "capability_test_required") ? <ActionButton actionId={`settings.agent.test.${agent.provider}`} icon={ShieldCheck} loading={pending === `agent.test.${agent.provider}`} onClick={() => testAgent(agent.provider)}>{pending === `agent.test.${agent.provider}` ? t.agentTesting : t.agentTest}</ActionButton> : null}</header>{agent.installations.map((installation) => <div className="vr-agent-channel" key={`${agent.provider}.${installation.channel}`}><code>{installation.channel === "local_service" ? t.localService : installation.channel === "desktop" ? t.desktop : t.cli}</code><span>{installation.version || capabilityLabel(installation, t)}</span><small>{AGENT_REASON[locale]?.[installation.reason_code] || installation.reason_code}</small>{installation.channel === "desktop" && installation.launchable ? <button type="button" data-action-id={`settings.agent.open.${agent.provider}`} onClick={() => tauriApi.launchAgentDesktop(agent.provider).catch((nextError) => setError(String(nextError)))}><Monitor />{t.agentOpen}</button> : null}</div>)}</section>)}</div></section>
    <section className="vr-settings-section"><header><FileJson /><div><h2>{t.diagnostics}</h2><p>{t.diagnosticsBody}</p></div></header><ActionButton actionId="settings.diagnostic.preview" icon={FileJson} loading={pending === "diagnostic.preview"} onClick={previewDiagnostic}>{t.previewDiagnostic}</ActionButton></section>
    <footer className="vr-settings-footer"><span>{t.trust}</span><button type="button" data-action-id="settings.open-source" onClick={() => tauriApi.openExternalUrl("https://github.com/logi-cmd/verity").catch(() => null)}>{t.openSource}<ExternalLink /></button></footer>
    {diagnostic ? <WindowPortal><div className="vr-diagnostic-overlay" role="dialog" aria-modal="true" aria-labelledby="diagnostic-title"><section className="vr-diagnostic-dialog"><header><div><h2 id="diagnostic-title">{t.diagnostics}</h2><p>{t.diagnosticsBody}</p></div><button type="button" data-action-id="diagnostic.close" aria-label={t.closePreview} onClick={() => setDiagnostic(null)}><X /></button></header><pre>{JSON.stringify(diagnostic, null, 2)}</pre>{diagnosticNotice ? <p className="vr-diagnostic-notice" role="status">{diagnosticNotice}</p> : null}<footer><ActionButton actionId="diagnostic.copy-issue" icon={Copy} onClick={copyIssueSummary}>{locale === "zh" ? "复制 Issue 摘要" : "Copy Issue summary"}</ActionButton><ActionButton actionId="diagnostic.open-issue" icon={ExternalLink} onClick={openIssue}>{t.issueDiagnostic}</ActionButton><ActionButton actionId="diagnostic.export" icon={Download} tone="primary" loading={pending === "diagnostic.export"} onClick={exportDiagnostic}>{t.exportDiagnostic}</ActionButton></footer></section></div></WindowPortal> : null}
  </div>;
}

export function App() {
  const [locale, setLocale] = usePreference("verity.locale.v1", "zh");
  const [motion, setMotion] = usePreference("verity.motion.v1", globalThis.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches ? "reduced" : "full");
  const [active, setActive] = useState("current");
  const [runtime, setRuntime] = useState(null);
  const [historyVersion, setHistoryVersion] = useState(0);
  const t = COPY[locale] || COPY.en;
  async function refreshRuntime() {
    setRuntime(tauriApi.available()
      ? await tauriApi.runtimeDoctor().catch(() => ({ status: "error", reason_code: "desktop_runtime_unavailable", launchable: false }))
      : { status: "error", reason_code: "desktop_runtime_unavailable", launchable: false });
  }
  useEffect(() => { void refreshRuntime(); }, [locale]);

  return <VerificationWorkspaceProvider t={t} onReceiptChanged={() => setHistoryVersion((value) => value + 1)}>
    <main className="vr-app" data-motion={motion}>
      <TitleBar />
      <Sidebar active={active} setActive={setActive} t={t} />
      <div className={`vr-scroll-owner ${active === "current" ? "is-current" : ""}`}>
        {active === "current" ? <CurrentCheckExperience locale={locale} t={t} runtime={runtime} motion={motion} /> : active === "history" ? <HistoryPage t={t} version={historyVersion} locale={locale} /> : <SettingsPage locale={locale} setLocale={setLocale} motion={motion} setMotion={setMotion} runtime={runtime} refreshRuntime={refreshRuntime} t={t} />}
      </div>
    </main>
  </VerificationWorkspaceProvider>;
}
