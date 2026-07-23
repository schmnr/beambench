export const MAX_FEEDBACK_TITLE_CHARS = 200;
export const MAX_FEEDBACK_DESCRIPTION_CHARS = 10_000;
export const MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS = 320;
export const MAX_PROJECT_ATTACHMENT_RAW_BYTES = 4_718_592;
export const MAX_SUBMIT_BODY_BYTES = 7_340_032;

import type { ConsoleEntry } from './console';
import type { JobProgress, MachineRunState, SessionState } from './machine';

export type FeedbackKind = 'bug' | 'connectivity' | 'crash';

export interface FeedbackSourceContext {
  source: string;
  error_message?: string | null;
  stack?: string | null;
  feature?: string | null;
  correlation_ts?: string | null;
}

export interface FeedbackReportInput {
  kind: FeedbackKind;
  title?: string | null;
  description?: string | null;
  notes?: string | null;
  reply_to_email?: string | null;
  include_project_file: boolean;
  source_context?: FeedbackSourceContext | null;
}

export interface DiagnosticClient {
  app_version: string;
  tauri_version?: string | null;
  rust_version?: string | null;
  build_target: string;
  git_sha: string;
}

export interface DiagnosticSystem {
  os: string;
  os_version?: string | null;
  arch: string;
  locale?: string | null;
}

export type DiagnosticSessionState =
  | 'connected'
  | 'disconnected'
  | 'connecting'
  | 'handshake_failed'
  | 'streaming'
  | 'error'
  | 'unknown';

export interface DiagnosticMachine {
  connected: boolean;
  model?: string | null;
  profile_id?: string | null;
  profile_name?: string | null;
  profile_preset_id?: string | null;
  profile_preset_version?: number | null;
  firmware_type?: string | null;
  controller_family?: string | null;
  controller_model?: string | null;
  transport_kind?: string | null;
  transfer_mode?: string | null;
  s_value_max?: number | null;
  homing_enabled?: boolean | null;
  use_constant_power?: boolean | null;
  emit_s_every_g1?: boolean | null;
  use_g0_for_overscan?: boolean | null;
  firmware_version?: string | null;
  baud_rate?: number | null;
  port_name?: string | null;
  port_vendor_id?: string | null;
  port_product_id?: string | null;
  session_state: DiagnosticSessionState;
  handshake_message?: string | null;
}

export interface DiagnosticPort {
  name: string;
  description?: string | null;
  manufacturer?: string | null;
  vendor_id?: string | null;
  product_id?: string | null;
  in_use_by_beambench: boolean;
  available: boolean;
}

export interface DiagnosticConnectionEvent {
  ts: string;
  stage: string;
  error_code?: string | null;
  port_name?: string | null;
  baud_rate?: number | null;
  message?: string | null;
  error?: string | null;
}

export interface DiagnosticSerialTraffic {
  tx_hex: string;
  tx_ascii: string;
  rx_hex: string;
  rx_ascii: string;
}

export interface DiagnosticLogEntry {
  ts: string;
  level: string;
  target: string;
  message: string;
}

export interface DiagnosticPanic {
  ts: string;
  thread?: string | null;
  message: string;
  location?: string | null;
  backtrace?: string | null;
  app_version: string;
  os: string;
  build_target: string;
  git_sha: string;
}

export interface DiagnosticProjectMetadata {
  object_count: number;
  size_bytes?: number | null;
  has_raster: boolean;
  has_vector: boolean;
  has_text: boolean;
  project_path?: string | null;
}

export interface KnownIssueWarning {
  code: string;
  severity: string;
  message: string;
}

export interface DiagnosticTerminalJob {
  captured_at: string;
  reason: string;
  progress?: JobProgress | null;
  error?: string | null;
  job_tick_loop_running: boolean;
  session_state?: SessionState | null;
  machine_run_state?: MachineRunState | null;
  job_console: ConsoleEntry[];
  session_console: ConsoleEntry[];
}

export interface DiagnosticBundleV1 {
  schema_version: number;
  kind: FeedbackKind;
  created_at: string;
  client: DiagnosticClient;
  system: DiagnosticSystem;
  machine: DiagnosticMachine;
  ports_detected: DiagnosticPort[];
  connection_events: DiagnosticConnectionEvent[];
  recent_serial: DiagnosticSerialTraffic;
  recent_logs: DiagnosticLogEntry[];
  recent_panics: DiagnosticPanic[];
  terminal_job?: DiagnosticTerminalJob | null;
  project_metadata?: DiagnosticProjectMetadata | null;
  known_issues: KnownIssueWarning[];
  project_file_attached: boolean;
  source_context?: FeedbackSourceContext | null;
}

export interface ConnectionDiagnosticsSnapshot {
  captured_at: string;
  ports_detected: DiagnosticPort[];
  machine: DiagnosticMachine;
  connection_events: DiagnosticConnectionEvent[];
  recent_serial: DiagnosticSerialTraffic;
  known_issues: KnownIssueWarning[];
}

export interface SavedReport {
  path: string;
  size_bytes: number;
}

export interface SubmitFeedbackResponse {
  report_id: string;
}

export const SUBMIT_FEEDBACK_DISCLOSURE_FIELDS = [
  'schema_version',
  'kind',
  'title',
  'description',
  'notes',
  'reply_to_email',
  'bundle',
  'project_file_attached',
  'project_file_blob',
] as const;

export const DIAGNOSTIC_BUNDLE_DISCLOSURE_FIELDS = [
  'schema_version',
  'kind',
  'created_at',
  'client',
  'system',
  'machine',
  'ports_detected',
  'connection_events',
  'recent_serial',
  'recent_logs',
  'recent_panics',
  'project_metadata',
  'known_issues',
  'project_file_attached',
  'source_context',
] as const;
