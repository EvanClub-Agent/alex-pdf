export type FileStatus = "ready" | "processing" | "success" | "failed";

export type FileItem = {
  id: string;
  file: File;
  name: string;
  size: number;
  status: FileStatus;
  selected: boolean;
  output?: string;
  error?: string;
  errorCode?: string;
  durationMs?: number;
};

export type Engine = "auto" | "wps" | "office";

export type EngineProbe = {
  engine: string;
  available: boolean;
  prog_id: string | null;
  version: string | null;
  error: string | null;
};

export type HealthResponse = {
  status: string;
  version: string;
  wps_available: boolean;
  wps_prog_id: string | null;
  engine: Engine;
};

export type CheckWpsResponse = {
  available: boolean;
  prog_id: string | null;
  version: string | null;
  error: string | null;
  engine: Engine;
  engines: EngineProbe[];
};

export type CollisionStrategy = "overwrite" | "auto_number" | "skip";

export type WatchFolder = {
  src: string;
  dst: string;
  delete_source: boolean;
};

export type SettingsResponse = {
  default_output_dir: string | null;
  conversion_timeout_secs: number;
  conversion_timeout_min_secs: number;
  conversion_timeout_max_secs: number;
  output_collision_strategy: CollisionStrategy;
  watch_folders: WatchFolder[];
};

export type DesktopSettingsResponse = {
  launch_on_startup: boolean;
  close_to_tray: boolean;
  silent_start_on_login: boolean;
  engine: Engine;
};

export type UpdateCheckResponse = {
  success: boolean;
  current_version: string;
  latest_version?: string | null;
  update_available: boolean;
  download_url?: string | null;
  notes?: string | null;
  pub_date?: string | null;
  sha256?: string | null;
  error?: string | null;
};

export type UpdateInstallResponse = {
  success: boolean;
  message: string;
};

export type UpdateStatus =
  | "idle"
  | "downloading"
  | "verifying"
  | "ready"
  | "launched"
  | "cancelled"
  | "failed";

export type UpdateProgress = {
  status: UpdateStatus;
  downloaded: number;
  total: number;
  installer_path?: string | null;
  error?: string | null;
  is_resume?: boolean;
};

export type PendingInstall = {
  expected_version: string;
  started_at_unix: number;
  installer_path: string;
};

export type PendingInstallResponse = {
  pending: PendingInstall | null;
  current_version: string;
  failed: boolean;
};

export type ConversionErrorCode =
  | "com_unavailable"
  | "file_not_found"
  | "password_protected"
  | "file_corrupted"
  | "unsupported_format"
  | "export_failed"
  | "timeout"
  | "cancelled"
  | "io_error"
  | "worker_panicked"
  | "unknown";

const ERROR_CODE_LABELS: Record<string, string> = {
  com_unavailable: "Office/WPS 不可用",
  file_not_found: "文件不存在",
  password_protected: "文档已加密",
  file_corrupted: "文件损坏",
  unsupported_format: "不支持的格式",
  export_failed: "导出 PDF 失败",
  timeout: "转换超时",
  cancelled: "已取消",
  io_error: "读写错误",
  worker_panicked: "工作线程异常",
  unknown: "转换失败"
};

const SHOULD_HIDE_RAW_DETAIL: ReadonlySet<string> = new Set([
  "unsupported_format",
  "password_protected",
  "file_corrupted",
  "file_not_found"
]);

/**
 * 把错误码 + 原始 message 拼成可读文案。
 * 对足够明确的 code（unsupported_format 等）不再拼接原始英文 message。
 */
export function formatConversionError(
  code: string | null | undefined,
  message: string | null | undefined
): string {
  const label = code ? ERROR_CODE_LABELS[code] ?? null : null;
  const detail = message?.trim() ?? "";
  if (label && code && SHOULD_HIDE_RAW_DETAIL.has(code)) return label;
  if (label && detail) return `${label}：${detail}`;
  if (label) return label;
  return detail !== "" ? detail : "转换失败";
}

export type FileResult = {
  file: string;
  status: string;
  output?: string | null;
  size?: number | null;
  duration_ms?: number | null;
  error?: string | null;
  error_code?: ConversionErrorCode | string | null;
};

export type SingleConvertResponse = {
  success: boolean;
  task_id?: string | null;
  input_file: string;
  output_file?: string | null;
  file_size?: number | null;
  duration_ms?: number | null;
  message: string;
  error?: string | null;
};

export type BatchConvertResponse = {
  success: boolean;
  task_id?: string | null;
  total: number;
  succeeded: number;
  failed: number;
  duration_ms?: number | null;
  results?: FileResult[] | null;
  message: string;
};

export type TaskStatusResponse = {
  task_id: string;
  status: "pending" | "processing" | "completed" | "failed" | "cancelled";
  progress: number;
  current_file?: string | null;
  total_files: number;
  completed_files: number;
  results?: FileResult[] | null;
  error?: string | null;
  error_code?: ConversionErrorCode | string | null;
  started_at: string;
};
