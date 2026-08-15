// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};

pub const RUN_PLAN_SCHEMA: &str = "verity-run-plan.v3";
pub const RUN_SESSION_SCHEMA: &str = "verity-run-session.v4";
pub const RECEIPT_SCHEMA: &str = "verity-verification-receipt.v3";
pub const RECEIPT_VERIFICATION_SCHEMA: &str = "verity-receipt-verification.v1";
pub const REMEDIATION_SCHEMA: &str = "verity-remediation-proposal.v1";
pub const AGENT_REPAIR_SCHEMA: &str = "verity-agent-repair.v2";
pub const RUNTIME_CAPABILITY_SCHEMA: &str = "verity-runtime-capability.v2";
pub const DIAGNOSTIC_REPORT_SCHEMA: &str = "verity-diagnostic-report.v1";
pub const CLEANUP_CANDIDATE_SCHEMA: &str = "verity-cleanup-candidate.v1";
pub const CLEANUP_PREVIEW_SCHEMA: &str = "verity-cleanup-preview.v1";
pub const CLEANUP_SESSION_SCHEMA: &str = "verity-cleanup-session.v1";
pub const CLEANUP_RECEIPT_SCHEMA: &str = "verity-cleanup-receipt.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStack {
    Node,
    Deno,
    Bun,
    StaticWeb,
    Rust,
    Python,
    Go,
    Godot,
    Compose,
    Java,
    Kotlin,
    C,
    Cpp,
    DotNet,
    Php,
    Ruby,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    Library,
    Cli,
    Web,
    Service,
    Desktop,
    Game,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Complete,
    Ambiguous,
    Incomplete,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentStatus {
    Ready,
    Missing,
    Incompatible,
    Unchecked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OracleStatus {
    Machine,
    Limited,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TargetRole {
    Product,
    Service,
    Tool,
    Component,
    Library,
    Example,
    Fixture,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockerOrigin {
    Repository,
    VerityPlan,
    Runtime,
    Oracle,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Planning,
    AwaitingConsent,
    Running,
    Blocked,
    Verified,
    StartedUnverified,
    Cancelled,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetResult {
    Verified,
    StartedUnverified,
    Blocked,
    Unsupported,
    NotRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Detect,
    Acquire,
    Build,
    Test,
    Launch,
    Oracle,
    Receipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunProgressEventKind {
    Started,
    Heartbeat,
    Observation,
    Completed,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    RegistryRestricted,
    InternalOnly,
    None,
    NativeUserConfirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OracleKind {
    TestSuite,
    PackageArtifact,
    HttpHtml,
    DeclaredHealth,
    DeclaredSmoke,
    WindowSignal,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandEvidence {
    pub path: String,
    pub key: String,
    pub precedence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedCommand {
    pub phase: RunPhase,
    pub program: String,
    pub args: Vec<String>,
    pub relative_cwd: String,
    pub evidence: CommandEvidence,
    pub network: NetworkPolicy,
    pub native: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationOracle {
    pub kind: OracleKind,
    pub description: String,
    pub machine_verifiable: bool,
    pub evidence: Vec<CommandEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanBlocker {
    pub phase: RunPhase,
    pub origin: BlockerOrigin,
    pub code: String,
    pub summary: String,
    pub detail: String,
    pub evidence: Vec<CommandEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetComponent {
    pub id: String,
    pub label: String,
    pub relative_root: String,
    pub stack: ProjectStack,
    pub kind: ProjectKind,
    pub role: TargetRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunTarget {
    pub id: String,
    pub label: String,
    pub relative_root: String,
    pub stack: ProjectStack,
    pub kind: ProjectKind,
    pub role: TargetRole,
    pub components: Vec<TargetComponent>,
    pub recommended: bool,
    pub selection_reason: String,
    pub plan_status: PlanStatus,
    pub environment_status: EnvironmentStatus,
    pub environment_reason_code: String,
    pub oracle_status: OracleStatus,
    pub commands: Vec<PlannedCommand>,
    pub oracle: VerificationOracle,
    pub blockers: Vec<PlanBlocker>,
    pub declarations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunPlan {
    pub schema: String,
    pub repository_root: String,
    pub repository_name: String,
    pub inspection_fingerprint: String,
    pub generated_at: String,
    pub targets: Vec<RunTarget>,
    pub ambiguity_count: usize,
    pub source_scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStatus {
    NotInstalled,
    Stopped,
    Starting,
    DaemonUnreachable,
    BuildkitUnavailable,
    CapabilityIncomplete,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Available,
    Unavailable,
    Unknown,
    NotChecked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityCheck {
    pub state: CapabilityState,
    pub version: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCapability {
    pub schema: String,
    pub provider: String,
    pub status: RuntimeStatus,
    pub installed: bool,
    pub launchable: bool,
    pub cli: CapabilityCheck,
    pub engine: CapabilityCheck,
    pub buildkit: CapabilityCheck,
    pub internal_network: CapabilityCheck,
    pub resource_limits: CapabilityCheck,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhaseReceipt {
    pub phase: RunPhase,
    pub command: Vec<String>,
    pub command_source: CommandEvidence,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
    pub network: NetworkPolicy,
    pub output_sha256: String,
    pub output_excerpt: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunProgress {
    pub phase: RunPhase,
    pub event_kind: RunProgressEventKind,
    pub completed_units: Option<u64>,
    pub total_units: Option<u64>,
    pub unit: Option<String>,
    pub indeterminate: bool,
    pub command: Vec<String>,
    pub command_source: Option<CommandEvidence>,
    pub working_directory: String,
    pub network: Option<NetworkPolicy>,
    pub execution_environment: String,
    pub started_at: String,
    pub elapsed_ms: u64,
    pub heartbeat_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunObservation {
    pub at: String,
    pub phase: RunPhase,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunProgressEvent {
    pub kind: RunProgressEventKind,
    pub progress: RunProgress,
    pub observation: Option<RunObservation>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OracleReceipt {
    pub kind: OracleKind,
    pub passed: bool,
    pub detail: String,
    pub evidence_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReceipt {
    pub schema: String,
    pub id: String,
    pub session_id: String,
    pub repository_name: String,
    pub repository_fingerprint: String,
    pub snapshot_fingerprint: String,
    pub target_id: String,
    pub target_label: String,
    pub target_relative_root: String,
    pub stack: ProjectStack,
    pub kind: ProjectKind,
    pub host_os: String,
    pub host_arch: String,
    pub execution_environment: String,
    pub toolchain: Vec<String>,
    pub runtime: RuntimeCapability,
    pub result: TargetResult,
    pub phases: Vec<PhaseReceipt>,
    pub oracle: OracleReceipt,
    pub first_observed_blocker: Option<PlanBlocker>,
    pub created_at: String,
    pub local_signature: String,
    pub local_public_key: String,
    pub signature_scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptVerification {
    pub schema: String,
    pub receipt_id: String,
    pub receipt_schema: String,
    pub result: TargetResult,
    pub signature_valid: bool,
    pub repository_fingerprint_matches: bool,
    pub snapshot_fingerprint_matches: bool,
    pub accepted: bool,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupCandidateKind {
    UnusedFile,
    DuplicateFile,
    UnusedDependency,
    UnusedSymbol,
    ObsoleteArtifact,
    UnreferencedResource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupEligibility {
    ReportOnly,
    ReverificationRequired,
    RemovalVerified,
    Protected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupCandidate {
    pub schema: String,
    pub id: String,
    pub kind: CleanupCandidateKind,
    pub path: String,
    pub related_path: Option<String>,
    pub size_bytes: u64,
    pub analyzer: String,
    pub evidence: Vec<String>,
    pub risk_reason: String,
    pub eligibility: CleanupEligibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupAnalyzerState {
    Completed,
    NotApplicable,
    NotInstalled,
    UnsafeConfiguration,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupAnalyzerStatus {
    pub analyzer: String,
    pub state: CleanupAnalyzerState,
    pub version: String,
    pub reason_code: String,
    pub finding_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupPreview {
    pub schema: String,
    pub candidates: Vec<CleanupCandidate>,
    pub analyzers: Vec<CleanupAnalyzerStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupSessionStatus {
    Planning,
    Analyzing,
    Revalidating,
    Completed,
    Cancelled,
    Blocked,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupSession {
    pub schema: String,
    pub id: String,
    pub repository_root: String,
    pub target_id: String,
    pub baseline_receipt_id: String,
    pub status: CleanupSessionStatus,
    pub candidates: Vec<CleanupCandidate>,
    pub verified_candidate_ids: Vec<String>,
    pub started_at: String,
    pub updated_at: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupBaseFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupReceipt {
    pub schema: String,
    pub id: String,
    pub cleanup_session_id: String,
    pub baseline_receipt_id: String,
    pub verification_receipt_id: String,
    pub target_id: String,
    pub candidate_ids: Vec<String>,
    pub removed_files: Vec<String>,
    pub removed_bytes: u64,
    pub unified_diff: String,
    pub base_files: Vec<CleanupBaseFile>,
    pub conclusion: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunSession {
    pub schema: String,
    pub id: String,
    pub status: SessionStatus,
    pub repository_root: String,
    pub target_id: String,
    pub current_phase: Option<RunPhase>,
    pub message: String,
    pub progress: Option<RunProgress>,
    pub phase_progress: Vec<RunProgress>,
    pub observations: Vec<RunObservation>,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub receipt_id: Option<String>,
    pub error: Option<String>,
    pub failure_origin: Option<BlockerOrigin>,
    pub failure_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemediationProposal {
    pub schema: String,
    pub id: String,
    pub blocker_code: String,
    pub summary: String,
    pub deterministic: bool,
    pub files: Vec<String>,
    pub requires_reverification: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentChannel {
    Cli,
    Desktop,
    LocalService,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapabilityStatus {
    NotInstalled,
    Detected,
    UnsupportedVersion,
    CapabilityTestRequired,
    Certified,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInstallation {
    pub channel: AgentChannel,
    pub status: AgentCapabilityStatus,
    pub version: String,
    pub entry_id: String,
    pub entry_hash: String,
    pub launchable: bool,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRepairCapability {
    pub schema: String,
    pub provider: String,
    pub installations: Vec<AgentInstallation>,
    pub non_interactive: bool,
    pub directory_confined: bool,
    pub structured_patch: bool,
    pub cancellable: bool,
    pub usable_in_app: bool,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRepairStatus {
    Pending,
    Running,
    Completed,
    Cancelled,
    Rejected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBaseFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRepairOutput {
    pub unified_diff: String,
    pub evidence: Vec<String>,
    pub base_files: Vec<AgentBaseFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRepairSession {
    pub schema: String,
    pub id: String,
    pub provider: String,
    pub status: AgentRepairStatus,
    pub target_id: String,
    pub started_at: String,
    pub updated_at: String,
    pub output: Option<AgentRepairOutput>,
    pub verification_result: Option<TargetResult>,
    pub receipt_id: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticAgentState {
    pub provider: String,
    pub channel: AgentChannel,
    pub status: AgentCapabilityStatus,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticResultCount {
    pub result: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticDurationBucket {
    pub phase: RunPhase,
    pub bucket: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticReport {
    pub schema: String,
    pub report_id: String,
    pub app_version: String,
    pub host_os: String,
    pub host_arch: String,
    pub runtime_status: RuntimeStatus,
    pub runtime_reason_code: String,
    pub agents: Vec<DiagnosticAgentState>,
    pub session_results: Vec<DiagnosticResultCount>,
    pub phase_durations: Vec<DiagnosticDurationBucket>,
    pub internal_error_codes: Vec<String>,
}
