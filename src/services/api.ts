import axios from "axios";
import { invoke } from "@tauri-apps/api/core";

import type {
  BatchConvertResponse,
  CheckWpsResponse,
  DesktopSettingsResponse,
  Engine,
  HealthResponse,
  PendingInstallResponse,
  SettingsResponse,
  SingleConvertResponse,
  TaskStatusResponse,
  UpdateCheckResponse,
  UpdateInstallResponse,
  UpdateProgress
} from "../types";

const client = axios.create({
  baseURL: "http://127.0.0.1:24568",
  timeout: 120000
});

export function setApiToken(token: string | null) {
  if (token) {
    client.defaults.headers.common["Authorization"] = `Bearer ${token}`;
  } else {
    delete client.defaults.headers.common["Authorization"];
  }
}

export type ApiTokenInfo = {
  enabled: boolean;
  token: string | null;
};

export async function getApiToken(): Promise<ApiTokenInfo> {
  return invoke<ApiTokenInfo>("get_api_token");
}

export async function setApiTokenEnabled(enabled: boolean): Promise<ApiTokenInfo> {
  return invoke<ApiTokenInfo>("set_api_token_enabled", { enabled });
}

export async function regenerateApiToken(): Promise<ApiTokenInfo> {
  return invoke<ApiTokenInfo>("regenerate_api_token");
}

export async function getHealth(): Promise<HealthResponse> {
  const { data } = await client.get<HealthResponse>("/health");
  return data;
}

export async function getSettings(): Promise<SettingsResponse> {
  const { data } = await client.get<SettingsResponse>("/api/settings");
  return data;
}

export async function updateSettings(
  payload: Partial<{
    conversion_timeout_secs: number;
    output_collision_strategy: import("../types").CollisionStrategy;
    watch_folders: import("../types").WatchFolder[];
  }>
): Promise<SettingsResponse> {
  const { data } = await client.put<SettingsResponse>("/api/settings", payload);
  return data;
}

export async function getDesktopSettings(): Promise<DesktopSettingsResponse> {
  return invoke<DesktopSettingsResponse>("get_desktop_settings");
}

export async function setLaunchOnStartup(enabled: boolean): Promise<DesktopSettingsResponse> {
  return invoke<DesktopSettingsResponse>("set_launch_on_startup", { enabled });
}

export async function setCloseToTray(enabled: boolean): Promise<DesktopSettingsResponse> {
  return invoke<DesktopSettingsResponse>("set_close_to_tray", { enabled });
}

export async function setSilentStartOnLogin(enabled: boolean): Promise<DesktopSettingsResponse> {
  return invoke<DesktopSettingsResponse>("set_silent_start_on_login", { enabled });
}

export async function setEngine(engine: Engine): Promise<DesktopSettingsResponse> {
  return invoke<DesktopSettingsResponse>("set_engine", { engine });
}

export async function hideMainWindow(): Promise<void> {
  await invoke("hide_main_window");
}

export async function revealInExplorer(path: string): Promise<void> {
  await invoke("reveal_in_explorer", { path });
}

export async function exitApplication(): Promise<void> {
  await invoke("exit_application");
}

export async function checkUpdate(): Promise<UpdateCheckResponse> {
  const { data } = await client.get<UpdateCheckResponse>("/api/update/check");
  return data;
}

export async function installUpdate(
  url: string,
  sha256?: string | null
): Promise<UpdateInstallResponse> {
  const { data } = await client.post<UpdateInstallResponse>(
    "/api/update/install",
    { url, sha256: sha256 ?? null },
    { timeout: 0 }
  );
  return data;
}

export async function getUpdateProgress(): Promise<UpdateProgress> {
  const { data } = await client.get<UpdateProgress>("/api/update/progress", {
    timeout: 5000
  });
  return data;
}

export async function launchUpdate(): Promise<UpdateInstallResponse> {
  const { data } = await client.post<UpdateInstallResponse>(
    "/api/update/launch",
    {},
    { timeout: 30000 }
  );
  return data;
}

export async function cancelUpdate(): Promise<UpdateInstallResponse> {
  const { data } = await client.post<UpdateInstallResponse>(
    "/api/update/cancel",
    {},
    { timeout: 5000 }
  );
  return data;
}

export async function getPendingInstall(): Promise<PendingInstallResponse> {
  const { data } = await client.get<PendingInstallResponse>(
    "/api/update/pending-install",
    { timeout: 5000 }
  );
  return data;
}

export async function clearPendingInstall(): Promise<UpdateInstallResponse> {
  const { data } = await client.post<UpdateInstallResponse>(
    "/api/update/clear-pending-install",
    {},
    { timeout: 5000 }
  );
  return data;
}

export async function checkWps(): Promise<CheckWpsResponse> {
  const { data } = await client.get<CheckWpsResponse>("/api/check-wps");
  return data;
}

export type EngineVersionResponse = {
  engine: Engine;
  available: boolean;
  version: string | null;
  error: string | null;
};

export async function getEngineVersion(engine: Engine): Promise<EngineVersionResponse> {
  const { data } = await client.get<EngineVersionResponse>("/api/engine-version", {
    params: { engine },
    timeout: 30000
  });
  return data;
}

export async function convertSingle(
  file: File,
  outputName?: string,
  asyncMode = true
): Promise<SingleConvertResponse> {
  const form = new FormData();
  form.append("file", file, file.name);
  if (outputName) {
    form.append("output_name", outputName);
  }

  const { data } = await client.post<SingleConvertResponse>("/api/convert", form, {
    params: asyncMode ? { async: "true" } : undefined
  });
  return data;
}

export async function convertFromUrl(
  url: string,
  outputName?: string,
  outputFolder?: string,
  asyncMode = false
): Promise<SingleConvertResponse> {
  const { data } = await client.post<SingleConvertResponse>(
    "/api/convert-from-url",
    {
      url,
      output_name: outputName || undefined,
      output_folder: outputFolder || undefined
    },
    {
      params: asyncMode ? { async: "true" } : undefined,
      timeout: 0
    }
  );
  return data;
}

export async function convertBatch(
  files: File[],
  outputFolder?: string,
  asyncMode = true
): Promise<BatchConvertResponse> {
  const form = new FormData();
  files.forEach((file) => form.append("files", file, file.name));
  if (outputFolder) {
    form.append("output_folder", outputFolder);
  }

  const { data } = await client.post<BatchConvertResponse>("/api/convert-batch", form, {
    params: asyncMode ? { async: "true" } : undefined
  });
  return data;
}

export async function convertBatchFromUrls(
  urls: string[],
  outputFolder?: string,
  asyncMode = true
): Promise<BatchConvertResponse> {
  const { data } = await client.post<BatchConvertResponse>(
    "/api/convert-batch-from-urls",
    {
      urls,
      output_folder: outputFolder || undefined
    },
    {
      params: asyncMode ? { async: "true" } : undefined,
      timeout: 0
    }
  );
  return data;
}

export async function getStatus(taskId: string): Promise<TaskStatusResponse> {
  const { data } = await client.get<TaskStatusResponse>(`/api/status/${taskId}`);
  return data;
}

export type CancelTaskResponse = {
  success: boolean;
  task_id: string;
  cancelled: boolean;
  message: string;
};

export async function cancelTask(taskId: string): Promise<CancelTaskResponse> {
  const { data } = await client.post<CancelTaskResponse>(
    `/api/cancel/${taskId}`,
    {},
    { timeout: 5000 }
  );
  return data;
}

export type ListTasksResponse = {
  total: number;
  tasks: TaskStatusResponse[];
};

export type DiagnosticsResponse = {
  version: string;
  engine: {
    configured: string;
    probes: Array<{
      engine: string;
      available: boolean;
      prog_id: string | null;
      version: string | null;
      error: string | null;
    }>;
  };
  tasks: {
    active: number;
    recent_failures: Array<{
      task_id: string;
      file: string;
      error: string | null;
      error_code: string | null;
      started_at: string;
    }>;
  };
  storage: {
    default_output_dir: string | null;
    fallback_output_dir: string;
    free_bytes: number | null;
  };
  security: { api_token_enabled: boolean };
  conversion_timeout_secs: number;
};

export async function getDiagnostics(): Promise<DiagnosticsResponse> {
  const { data } = await client.get<DiagnosticsResponse>("/api/diagnostics", { timeout: 10000 });
  return data;
}

export type QueueResponse = {
  counts: {
    pending: number;
    processing: number;
    completed: number;
    failed: number;
    cancelled: number;
  };
  active: TaskStatusResponse[];
};

export async function getQueue(): Promise<QueueResponse> {
  const { data } = await client.get<QueueResponse>("/api/queue", { timeout: 5000 });
  return data;
}

export async function listTasks(params: {
  limit?: number;
  status?: string;
} = {}): Promise<ListTasksResponse> {
  const { data } = await client.get<ListTasksResponse>("/api/tasks", {
    params,
    timeout: 10000
  });
  return data;
}
