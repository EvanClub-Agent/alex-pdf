import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import clsx from "clsx";
import {
  ArrowUp,
  Check,
  CheckCircle2,
  CheckSquare,
  ChevronDown,
  Copy,
  Download,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  Globe,
  Layers,
  Link2,
  Monitor,
  Moon,
  Plus,
  RefreshCw,
  RotateCcw,
  Shield,
  Square,
  Sun,
  Terminal,
  Trash2,
  XCircle
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { open as openShell } from "@tauri-apps/plugin-shell";

import { FileDropZone } from "./components/FileDropZone";
import { FileList } from "./components/FileList";
import { AppIcon } from "./components/AppIcon";
import { LogPanel } from "./components/LogPanel";
import { ModalDialog } from "./components/ModalDialog";
import { NavigationRail, type ComposerMode, type NavKey } from "./components/NavigationRail";
import { ProgressBar } from "./components/ProgressBar";
import { StatusBar } from "./components/StatusBar";
import { useConversion } from "./hooks/useConversion";
import { useFileUpload } from "./hooks/useFileUpload";
import {
  cancelUpdate,
  checkUpdate,
  checkWps,
  clearPendingInstall,
  convertBatchFromUrls,
  convertFromUrl,
  exitApplication,
  getApiToken,
  getDesktopSettings,
  getDiagnostics,
  getEngineVersion,
  getHealth,
  getPendingInstall,
  getQueue,
  getSettings,
  getStatus,
  getUpdateProgress,
  hideMainWindow,
  installUpdate,
  launchUpdate,
  listTasks,
  regenerateApiToken,
  revealInExplorer,
  setApiToken,
  setApiTokenEnabled,
  setCloseToTray,
  setEngine,
  setLaunchOnStartup,
  setSilentStartOnLogin,
  updateSettings,
  type DiagnosticsResponse,
  type QueueResponse
} from "./services/api";
import type {
  CheckWpsResponse,
  DesktopSettingsResponse,
  Engine,
  HealthResponse,
  PendingInstallResponse,
  SettingsResponse,
  SingleConvertResponse,
  TaskStatusResponse,
  UpdateCheckResponse,
  UpdateProgress
} from "./types";
import { formatConversionError } from "./types";

const SERVER_PORT = 24568;

type ThemePreference = "system" | "light" | "dark";

function readInitialThemePreference(): ThemePreference {
  try {
    const raw = localStorage.getItem("color-scheme");
    if (raw === "light" || raw === "dark") return raw;
  } catch {
    /* ignore */
  }
  return "system";
}

function applyThemeMeta(pref: ThemePreference) {
  const meta = document.querySelector('meta[name="color-scheme"]');
  if (meta) {
    meta.setAttribute("content", pref === "system" ? "light dark" : pref);
  }
}

function getDirectory(path: string) {
  const slash = path.lastIndexOf("/");
  const backslash = path.lastIndexOf("\\");
  const index = Math.max(slash, backslash);
  if (index <= 0) return "";
  return path.slice(0, index);
}

function getBasename(path: string) {
  const cleaned = path.replace(/[/\\]+$/, "");
  const slash = cleaned.lastIndexOf("/");
  const backslash = cleaned.lastIndexOf("\\");
  const index = Math.max(slash, backslash);
  return index >= 0 ? cleaned.slice(index + 1) || cleaned : cleaned;
}

function formatMB(bytes: number) {
  if (!bytes) return "0 MB";
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatBytes(bytes: number | null | undefined) {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

function formatDate(raw: string) {
  try {
    return new Date(raw).toLocaleString();
  } catch {
    return raw;
  }
}

const engineLabels: Record<Engine, string> = {
  auto: "自动检测",
  wps: "金山 WPS",
  office: "Microsoft Office"
};

const statusTextMap: Record<TaskStatusResponse["status"], string> = {
  pending: "排队中",
  processing: "转换中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消"
};

function App() {
  const {
    files,
    addFiles,
    clearFiles,
    removeFile,
    toggleSelect,
    selectAll,
    updateFiles,
    selectedCount
  } = useFileUpload();

  const {
    status,
    progress,
    currentFile,
    totalFiles,
    completedFiles,
    logs,
    error,
    reset,
    startBatch,
    cancel,
    cancelling
  } = useConversion(files, updateFiles);

  /* Navigation & Layout State */
  const [activeNav, setActiveNav] = useState<NavKey>("workspace");
  const [composerMode, setComposerMode] = useState<ComposerMode>("local");
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(() => {
    try {
      return localStorage.getItem("ui.sidebarCollapsed") === "true";
    } catch {
      return false;
    }
  });
  const [outputEditorOpen, setOutputEditorOpen] = useState(false);

  /* Theme Preference (follows dark-mode-toggle standard) */
  const [themePref, setThemePref] = useState<ThemePreference>(readInitialThemePreference);
  const [systemDark, setSystemDark] = useState<boolean>(() =>
    typeof window !== "undefined"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
      : false
  );

  /* Service & Settings State */
  const [outputFolder, setOutputFolder] = useState<string>("");
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [wpsInfo, setWpsInfo] = useState<CheckWpsResponse | null>(null);
  const [desktopSettings, setDesktopSettings] = useState<DesktopSettingsResponse | null>(null);
  const [serverSettings, setServerSettings] = useState<SettingsResponse | null>(null);
  const [desktopAction, setDesktopAction] = useState<string | null>(null);

  const [timeoutInput, setTimeoutInput] = useState<string>("");
  const [timeoutSaving, setTimeoutSaving] = useState(false);
  const [timeoutError, setTimeoutError] = useState<string | null>(null);

  const [tokenInfo, setTokenInfo] = useState<{ enabled: boolean; token: string | null } | null>(
    null
  );
  const [tokenAction, setTokenAction] = useState<string | null>(null);
  const [tokenCopied, setTokenCopied] = useState(false);

  /* History & Diagnostics State */
  const [historyTasks, setHistoryTasks] = useState<TaskStatusResponse[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [historyFilter, setHistoryFilter] = useState<string>("all");

  const [diagnostics, setDiagnostics] = useState<DiagnosticsResponse | null>(null);
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);
  const [queue, setQueue] = useState<QueueResponse | null>(null);

  /* URL & Batch URL Conversion State */
  const [remoteUrl, setRemoteUrl] = useState("");
  const [remoteOutputName, setRemoteOutputName] = useState("");
  const [remoteConverting, setRemoteConverting] = useState(false);
  const [remoteResult, setRemoteResult] = useState<SingleConvertResponse | null>(null);
  const [remoteError, setRemoteError] = useState<string | null>(null);

  const [remoteBatchUrls, setRemoteBatchUrls] = useState("");
  const [remoteBatchTaskId, setRemoteBatchTaskId] = useState<string | null>(null);
  const [remoteBatchStatus, setRemoteBatchStatus] = useState<TaskStatusResponse | null>(null);
  const [remoteBatchConverting, setRemoteBatchConverting] = useState(false);
  const [remoteBatchError, setRemoteBatchError] = useState<string | null>(null);

  /* Preview, Engine Version & Update State */
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [engineVersions, setEngineVersions] = useState<Record<string, string | null>>({});
  const [engineVersionLoading, setEngineVersionLoading] = useState<string | null>(null);

  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResponse | null>(null);
  const [updateVisible, setUpdateVisible] = useState(false);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [updateCancelling, setUpdateCancelling] = useState(false);
  const [pendingInstall, setPendingInstall] = useState<PendingInstallResponse | null>(null);
  const [pendingInstallVisible, setPendingInstallVisible] = useState(false);
  const [skippedVersion, setSkippedVersion] = useState<string | null>(() => {
    try {
      return localStorage.getItem("update.skippedVersion");
    } catch {
      return null;
    }
  });

  const engineDropdownRef = useRef<HTMLDetailsElement | null>(null);

  /* Theme & Media Query Sync */
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mq.addEventListener("change", onChange);

    const onStorage = (e: StorageEvent) => {
      if (e.key === "color-scheme") {
        const next: ThemePreference =
          e.newValue === "light" || e.newValue === "dark" ? e.newValue : "system";
        setThemePref(next);
        applyThemeMeta(next);
      }
    };
    window.addEventListener("storage", onStorage);

    return () => {
      mq.removeEventListener("change", onChange);
      window.removeEventListener("storage", onStorage);
    };
  }, []);

  const handleSelectTheme = (next: ThemePreference) => {
    setThemePref(next);
    applyThemeMeta(next);
    try {
      if (next === "system") {
        localStorage.removeItem("color-scheme");
      } else {
        localStorage.setItem("color-scheme", next);
      }
    } catch {
      /* ignore storage errors */
    }
  };

  /* Close topbar engine dropdown when clicking outside */
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const el = engineDropdownRef.current;
      if (el?.open && event.target instanceof Node && !el.contains(event.target)) {
        el.open = false;
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, []);

  const refreshHistory = useCallback(async (silent = false) => {
    if (!silent) {
      setHistoryLoading(true);
      setHistoryError(null);
    }
    try {
      const result = await listTasks({ limit: 100 });
      setHistoryTasks(result.tasks);
    } catch (err) {
      if (!silent) {
        setHistoryError(err instanceof Error ? err.message : "加载历史失败");
      }
    } finally {
      if (!silent) setHistoryLoading(false);
    }
  }, []);

  /* Bootstrap & Health Polling */
  useEffect(() => {
    let active = true;

    const bootstrap = async () => {
      try {
        const info = await getApiToken();
        if (!active) return;
        setTokenInfo(info);
        setApiToken(info.enabled ? info.token : null);
      } catch {
        if (active) setApiToken(null);
      }

      try {
        const [settings, desktopInfo] = await Promise.all([getSettings(), getDesktopSettings()]);
        if (!active) return;
        if (settings.default_output_dir) {
          setOutputFolder((prev) => (prev ? prev : settings.default_output_dir!));
        }
        setServerSettings(settings);
        setTimeoutInput(String(settings.conversion_timeout_secs));
        setDesktopSettings(desktopInfo);
      } catch {
        /* UI handles empty state gracefully */
      }

      void refreshHistory(true);
    };

    const loadHealth = async () => {
      try {
        const healthInfo = await getHealth();
        if (active) setHealth(healthInfo);
      } catch {
        if (active) setHealth(null);
      }
    };

    bootstrap().then(loadHealth);
    const timer = setInterval(loadHealth, 5000);

    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [refreshHistory]);

  /* Refresh sidebar recent list when local conversion finishes */
  useEffect(() => {
    if (status === "completed" || status === "failed" || status === "cancelled") {
      void refreshHistory(true);
    }
  }, [status, refreshHistory]);

  /* Close-to-tray behavior */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const currentWindow = getCurrentWindow();

    currentWindow
      .onCloseRequested(async (event) => {
        if (!desktopSettings?.close_to_tray) return;
        event.preventDefault();
        await hideMainWindow();
      })
      .then((cleanup) => {
        unlisten = cleanup;
      })
      .catch((err) => console.error(err));

    return () => {
      unlisten?.();
    };
  }, [desktopSettings?.close_to_tray]);

  /* Startup WPS probe, update check & pending-install check */
  useEffect(() => {
    let active = true;

    const runUpdateCheck = async () => {
      const lastCheck = Number(localStorage.getItem("update.lastCheckAt") ?? "0");
      const oneDay = 24 * 60 * 60 * 1000;
      if (lastCheck && Date.now() - lastCheck < oneDay) return;

      setUpdateChecking(true);
      try {
        const result = await checkUpdate();
        if (!active) return;
        setUpdateInfo(result);
        if (result.success) {
          localStorage.setItem("update.lastCheckAt", String(Date.now()));
        }
      } finally {
        if (active) setUpdateChecking(false);
      }
    };

    const loadWps = async () => {
      try {
        const info = await checkWps();
        if (active) setWpsInfo(info);
      } catch {
        if (active) setWpsInfo(null);
      }
    };

    const detectPendingInstall = async () => {
      try {
        const pending = await getPendingInstall();
        if (!active) return;
        setPendingInstall(pending);
        if (pending.failed) {
          setPendingInstallVisible(true);
        } else if (pending.pending) {
          await clearPendingInstall();
        }
      } catch {
        /* ignore */
      }
    };

    runUpdateCheck();
    loadWps();
    detectPendingInstall();

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (activeNav === "history") {
      void refreshHistory();
    }
  }, [activeNav, refreshHistory]);

  const refreshDiagnostics = useCallback(async () => {
    setDiagnosticsLoading(true);
    setDiagnosticsError(null);
    try {
      const result = await getDiagnostics();
      setDiagnostics(result);
    } catch (err) {
      setDiagnosticsError(err instanceof Error ? err.message : "加载诊断失败");
    } finally {
      setDiagnosticsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (activeNav !== "diagnostics") return;
    void refreshDiagnostics();
    let active = true;
    const refreshQ = async () => {
      try {
        const q = await getQueue();
        if (active) setQueue(q);
      } catch {
        /* ignore */
      }
    };
    void refreshQ();
    const t = setInterval(refreshQ, 2000);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, [activeNav, refreshDiagnostics]);

  /* Update download polling */
  useEffect(() => {
    if (!updateProgress) return;
    if (updateProgress.status !== "downloading" && updateProgress.status !== "verifying") {
      return;
    }
    let active = true;
    const timer = setInterval(async () => {
      try {
        const next = await getUpdateProgress();
        if (!active || next.status === "idle") return;
        setUpdateProgress(next);
        if (next.status === "ready" || next.status === "failed" || next.status === "launched") {
          clearInterval(timer);
        }
      } catch {
        /* retry next tick */
      }
    }, 600);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [updateProgress?.status]);

  /* Batch URL task polling */
  useEffect(() => {
    if (!remoteBatchTaskId) return;

    let active = true;
    const timer = setInterval(async () => {
      try {
        const result = await getStatus(remoteBatchTaskId);
        if (!active) return;
        setRemoteBatchStatus(result);
        if (
          result.status === "completed" ||
          result.status === "failed" ||
          result.status === "cancelled"
        ) {
          setRemoteBatchConverting(false);
          setRemoteBatchTaskId(null);
          clearInterval(timer);
          void refreshHistory(true);
        }
      } catch (err) {
        if (!active) return;
        const message = err instanceof Error ? err.message : "批量链接任务查询失败";
        setRemoteBatchError(message);
        setRemoteBatchConverting(false);
        setRemoteBatchTaskId(null);
        clearInterval(timer);
      }
    }, 1000);

    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [remoteBatchTaskId, refreshHistory]);

  /* Derived Helpers */
  const allSelected = useMemo(
    () => files.length > 0 && selectedCount === files.length,
    [files.length, selectedCount]
  );

  const inferredOutputFolder = useMemo(() => {
    for (let i = files.length - 1; i >= 0; i -= 1) {
      const output = files[i].output;
      if (output) return getDirectory(output);
    }
    return "";
  }, [files]);

  const parsedBatchUrlCount = useMemo(
    () =>
      remoteBatchUrls
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean).length,
    [remoteBatchUrls]
  );

  const currentEngine: Engine = desktopSettings?.engine ?? health?.engine ?? "auto";
  const engineAvailable = health?.wps_available ?? wpsInfo?.available ?? false;

  const isWorkspaceFresh =
    (composerMode === "local" && files.length === 0 && status === "idle") ||
    (composerMode === "url" && !remoteUrl.trim() && !remoteResult && !remoteError) ||
    (composerMode === "batch" &&
      parsedBatchUrlCount === 0 &&
      !remoteBatchStatus &&
      !remoteBatchError);

  /* Event Handlers */
  const handleToggleSidebar = () => {
    setSidebarCollapsed((prev) => {
      const next = !prev;
      try {
        localStorage.setItem("ui.sidebarCollapsed", String(next));
      } catch {
        /* ignore */
      }
      return next;
    });
  };

  const handleNewConversion = () => {
    setActiveNav("workspace");
    if (status !== "processing") {
      clearFiles();
      reset();
    }
    if (!remoteConverting) {
      setRemoteUrl("");
      setRemoteOutputName("");
      setRemoteResult(null);
      setRemoteError(null);
    }
    if (!remoteBatchConverting) {
      setRemoteBatchUrls("");
      setRemoteBatchStatus(null);
      setRemoteBatchError(null);
    }
  };

  const handleSelectWorkspaceMode = (nextMode: ComposerMode) => {
    setActiveNav("workspace");
    setComposerMode(nextMode);
  };

  const handlePickOutput = async () => {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "选择 PDF 输出目录"
      });
      if (typeof selected === "string") {
        setOutputFolder(selected);
      } else if (Array.isArray(selected) && selected[0]) {
        setOutputFolder(selected[0]);
      }
    } catch (err) {
      console.error(err);
      window.alert("无法打开目录选择器，请确认已授权 dialog 权限。");
    }
  };

  const handlePickFolderInto = async (onPick: (folder: string) => void) => {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "选择文件夹"
      });
      if (typeof selected === "string") {
        onPick(selected);
      } else if (Array.isArray(selected) && selected[0]) {
        onPick(selected[0]);
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleOpenFolder = async () => {
    const target = outputFolder || inferredOutputFolder;
    if (!target) return;
    try {
      await revealInExplorer(target);
    } catch (err) {
      console.error(err);
      window.alert(`无法打开输出目录: ${err}`);
    }
  };

  const handleOpenOutputDir = async (output: string) => {
    const target = getDirectory(output) || outputFolder || inferredOutputFolder;
    if (!target) {
      window.alert("尚未设置输出目录");
      return;
    }
    try {
      await revealInExplorer(output || target);
    } catch {
      try {
        await revealInExplorer(target);
      } catch (err) {
        window.alert(`无法打开输出目录: ${err}`);
      }
    }
  };

  const handleClear = () => {
    clearFiles();
    reset();
  };

  const handleRetryFailed = () => {
    updateFiles((prev) =>
      prev.map((item) =>
        item.status === "failed"
          ? { ...item, status: "ready", selected: true, error: undefined, errorCode: undefined }
          : { ...item, selected: false }
      )
    );
    void startBatch(outputFolder || undefined);
  };

  const handleRefresh = async () => {
    try {
      const [healthInfo, wpsDetail] = await Promise.all([getHealth(), checkWps()]);
      setHealth(healthInfo);
      setWpsInfo(wpsDetail);
    } catch {
      setHealth(null);
    }
  };

  const handleSelectEngine = async (engine: Engine) => {
    setDesktopAction(`engine-${engine}`);
    if (engineDropdownRef.current) {
      engineDropdownRef.current.open = false;
    }
    try {
      const result = await setEngine(engine);
      setDesktopSettings(result);
      try {
        const [healthInfo, wpsDetail] = await Promise.all([getHealth(), checkWps()]);
        setHealth(healthInfo);
        setWpsInfo(wpsDetail);
      } catch {
        /* ignore */
      }
    } catch (err) {
      console.error(err);
      window.alert("切换转换引擎失败");
    } finally {
      setDesktopAction(null);
    }
  };

  const handleFetchEngineVersion = async (engine: Engine) => {
    setEngineVersionLoading(engine);
    try {
      const result = await getEngineVersion(engine);
      setEngineVersions((prev) => ({ ...prev, [engine]: result.version }));
    } catch (err) {
      window.alert(`获取版本失败: ${err}`);
    } finally {
      setEngineVersionLoading(null);
    }
  };

  const handleConvertFromUrl = async () => {
    if (!remoteUrl.trim()) {
      setRemoteError("请输入文档下载链接");
      return;
    }
    setRemoteConverting(true);
    setRemoteError(null);
    setRemoteResult(null);
    try {
      const result = await convertFromUrl(
        remoteUrl.trim(),
        remoteOutputName.trim() || undefined,
        outputFolder || undefined
      );
      setRemoteResult(result);
      if (!result.success) {
        setRemoteError(result.error || result.message || "链接转换失败");
      }
      void refreshHistory(true);
    } catch (err) {
      setRemoteError(err instanceof Error ? err.message : "链接转换失败");
    } finally {
      setRemoteConverting(false);
    }
  };

  const handleBatchConvertFromUrls = async () => {
    const urls = remoteBatchUrls
      .split(/\r?\n/)
      .map((item) => item.trim())
      .filter(Boolean);

    if (!urls.length) {
      setRemoteBatchError("请输入至少一个文档下载链接");
      return;
    }

    setRemoteBatchConverting(true);
    setRemoteBatchError(null);
    setRemoteBatchStatus(null);
    setRemoteBatchTaskId(null);
    try {
      const result = await convertBatchFromUrls(urls, outputFolder || undefined, true);
      if (!result.task_id) {
        throw new Error("未获取到任务 ID");
      }
      setRemoteBatchTaskId(result.task_id);
    } catch (err) {
      setRemoteBatchError(err instanceof Error ? err.message : "批量链接转换失败");
      setRemoteBatchConverting(false);
    }
  };

  const handleRetryRemoteUrl = async (fileName: string) => {
    const urls = remoteBatchUrls
      .split(/\r?\n/)
      .map((item) => item.trim())
      .filter(Boolean);
    const matched =
      urls.find((url) => url.split(/[/\\?#]/).filter(Boolean).pop() === fileName) ??
      urls.find((url) => url.includes(fileName));
    if (!matched) {
      window.alert("未能定位原始链接，请手动重新粘贴");
      return;
    }
    setRemoteBatchConverting(true);
    setRemoteBatchError(null);
    try {
      const result = await convertFromUrl(matched, undefined, outputFolder || undefined, false);
      setRemoteBatchStatus((prev) => {
        if (!prev?.results) return prev;
        const next = prev.results.map((row) =>
          row.file === fileName
            ? {
                ...row,
                status: result.success ? "success" : "failed",
                output: result.output_file ?? null,
                error: result.error ?? null,
                error_code: null
              }
            : row
        );
        return { ...prev, results: next };
      });
    } catch {
      window.alert("重试失败");
    } finally {
      setRemoteBatchConverting(false);
    }
  };

  /* Settings & Token Handlers */
  const handleToggleLaunchOnStartup = async (enabled: boolean) => {
    setDesktopAction("launch");
    try {
      const result = await setLaunchOnStartup(enabled);
      setDesktopSettings(result);
    } catch {
      window.alert("设置开机自启失败");
    } finally {
      setDesktopAction(null);
    }
  };

  const handleToggleCloseToTray = async (enabled: boolean) => {
    setDesktopAction("closeToTray");
    try {
      const result = await setCloseToTray(enabled);
      setDesktopSettings(result);
    } catch {
      window.alert("设置关闭到托盘失败");
    } finally {
      setDesktopAction(null);
    }
  };

  const handleToggleSilentStart = async (enabled: boolean) => {
    setDesktopAction("silentStart");
    try {
      const result = await setSilentStartOnLogin(enabled);
      setDesktopSettings(result);
    } catch {
      window.alert("设置静默启动失败");
    } finally {
      setDesktopAction(null);
    }
  };

  const handleToggleApiToken = async (enabled: boolean) => {
    setTokenAction("toggle");
    try {
      const info = await setApiTokenEnabled(enabled);
      setTokenInfo(info);
      setApiToken(info.enabled ? info.token : null);
    } catch {
      window.alert("更新 API Token 失败");
    } finally {
      setTokenAction(null);
    }
  };

  const handleRegenerateApiToken = async () => {
    setTokenAction("regenerate");
    try {
      const info = await regenerateApiToken();
      setTokenInfo(info);
      setApiToken(info.token);
    } catch {
      window.alert("重新生成 API Token 失败");
    } finally {
      setTokenAction(null);
    }
  };

  const handleCopyApiToken = async () => {
    if (!tokenInfo?.token) return;
    try {
      await navigator.clipboard.writeText(tokenInfo.token);
      setTokenCopied(true);
      window.setTimeout(() => setTokenCopied(false), 1800);
    } catch (err) {
      console.error(err);
    }
  };

  const handleSaveTimeout = async () => {
    if (!serverSettings) return;
    const parsed = Number(timeoutInput);
    const min = serverSettings.conversion_timeout_min_secs;
    const max = serverSettings.conversion_timeout_max_secs;
    if (!Number.isFinite(parsed) || !Number.isInteger(parsed) || parsed < min || parsed > max) {
      setTimeoutError(`请输入 ${min} - ${max} 之间的整数`);
      return;
    }
    setTimeoutSaving(true);
    setTimeoutError(null);
    try {
      const result = await updateSettings({ conversion_timeout_secs: parsed });
      setServerSettings(result);
      setTimeoutInput(String(result.conversion_timeout_secs));
    } catch {
      setTimeoutError("保存失败");
    } finally {
      setTimeoutSaving(false);
    }
  };

  const handleHideToTray = async () => {
    try {
      await hideMainWindow();
    } catch {
      window.alert("隐藏到托盘失败");
    }
  };

  /* Update Handlers */
  const handleVersionChipClick = () => {
    if (
      updateInfo?.update_available &&
      updateInfo.latest_version &&
      updateInfo.latest_version !== skippedVersion
    ) {
      setUpdateVisible(true);
      return;
    }
    void handleCheckUpdate();
  };

  const handleCheckUpdate = async () => {
    setUpdateChecking(true);
    try {
      const result = await checkUpdate();
      setUpdateInfo(result);
      if (result.success) {
        localStorage.setItem("update.lastCheckAt", String(Date.now()));
      }
      if (result.success && result.update_available) {
        setUpdateVisible(true);
        return;
      }
      if (!result.success) {
        window.alert(result.error || "检查更新失败");
        return;
      }
      window.alert("当前已是最新版本");
    } finally {
      setUpdateChecking(false);
    }
  };

  const handleInstallUpdate = async () => {
    if (!updateInfo?.download_url) {
      window.alert("未获取到下载地址");
      return;
    }
    setUpdateInstalling(true);
    setUpdateProgress({ status: "downloading", downloaded: 0, total: 0 });
    try {
      const result = await installUpdate(updateInfo.download_url, updateInfo.sha256);
      if (!result.success) {
        window.alert(result.message);
        setUpdateProgress({
          status: "failed",
          downloaded: 0,
          total: 0,
          error: result.message
        });
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      window.alert(`启动下载失败: ${message}`);
      setUpdateProgress({
        status: "failed",
        downloaded: 0,
        total: 0,
        error: message
      });
    } finally {
      setUpdateInstalling(false);
    }
  };

  const handleLaunchUpdate = async () => {
    setUpdateInstalling(true);
    try {
      const result = await launchUpdate();
      if (!result.success) {
        window.alert(result.message);
        return;
      }
      setUpdateVisible(false);
      await new Promise((resolve) => setTimeout(resolve, 1500));
      await exitApplication();
    } finally {
      setUpdateInstalling(false);
    }
  };

  const handleCancelUpdate = async () => {
    setUpdateCancelling(true);
    try {
      const result = await cancelUpdate();
      if (!result.success) {
        window.alert(result.message);
      }
    } finally {
      setUpdateCancelling(false);
    }
  };

  const handleSkipVersion = () => {
    if (updateInfo?.latest_version) {
      try {
        localStorage.setItem("update.skippedVersion", updateInfo.latest_version);
      } catch {
        /* ignore */
      }
      setSkippedVersion(updateInfo.latest_version);
    }
    setUpdateVisible(false);
  };

  const handleAcknowledgePendingFailure = async () => {
    try {
      await clearPendingInstall();
    } catch {
      /* ignore */
    }
    setPendingInstall(null);
    setPendingInstallVisible(false);
  };

  /* Engine probe metadata */
  const engines = wpsInfo?.engines ?? [];
  const wpsProbe = engines.find((item) => item.engine === "wps");
  const officeProbe = engines.find((item) => item.engine === "office");

  const engineOptions: Array<{
    key: Engine;
    title: string;
    desc: string;
    available: boolean;
    version?: string | null;
  }> = [
    {
      key: "auto",
      title: "自动检测",
      desc: "优先调用金山 WPS，未安装则自动回退 Microsoft Office",
      available: (wpsProbe?.available ?? false) || (officeProbe?.available ?? false)
    },
    {
      key: "wps",
      title: "金山 WPS",
      desc: "使用本机 WPS Office COM 接口导出 PDF",
      available: wpsProbe?.available ?? false,
      version: engineVersions["wps"] ?? wpsProbe?.version ?? null
    },
    {
      key: "office",
      title: "Microsoft Office",
      desc: "使用本机 Word / Excel COM 接口导出 PDF",
      available: officeProbe?.available ?? false,
      version: engineVersions["office"] ?? officeProbe?.version ?? null
    }
  ];

  /* ==================== Views ==================== */

  const renderWorkspaceView = () => {
    const outputLabel = outputFolder ? getBasename(outputFolder) : "默认输出目录";

    return (
      <div className={clsx("workspace-column", isWorkspaceFresh && "hero-centered")}>
        {isWorkspaceFresh ? (
          <>
            <section className="gemini-hero" aria-label="欢迎">
              <h1 className="gemini-hero-greeting">你好，准备好转换文档了吗？</h1>
              <p className="gemini-hero-subtitle">
                纯本地 WPS / Office 内核驱动，文档不出本机，即刻生成高保真 PDF
              </p>
            </section>

            <section className="suggestion-grid" aria-label="快捷转换方式">
              <button
                type="button"
                className={clsx("suggestion-card", composerMode === "local" && "active")}
                onClick={() => setComposerMode("local")}
              >
                <div>
                  <div className="suggestion-card-title">本地文档转 PDF</div>
                  <div className="suggestion-card-desc">
                    拖入一个或多个 Word (.doc/.docx) 与 Excel (.xls/.xlsx) 文件批量导出
                  </div>
                </div>
                <span className="suggestion-card-icon" aria-hidden="true">
                  <FileText size={17} />
                </span>
              </button>

              <button
                type="button"
                className={clsx("suggestion-card", composerMode === "url" && "active")}
                onClick={() => setComposerMode("url")}
              >
                <div>
                  <div className="suggestion-card-title">单条链接下载转换</div>
                  <div className="suggestion-card-desc">
                    粘贴远程 Office 文档直链，自动拉取并输出为指定名称的 PDF
                  </div>
                </div>
                <span className="suggestion-card-icon" aria-hidden="true">
                  <Link2 size={17} />
                </span>
              </button>

              <button
                type="button"
                className={clsx("suggestion-card", composerMode === "batch" && "active")}
                onClick={() => setComposerMode("batch")}
              >
                <div>
                  <div className="suggestion-card-title">批量链接队列</div>
                  <div className="suggestion-card-desc">
                    每行一条文档下载链接，后台顺序转换并实时追踪每项状态
                  </div>
                </div>
                <span className="suggestion-card-icon" aria-hidden="true">
                  <Layers size={17} />
                </span>
              </button>
            </section>
          </>
        ) : null}

        {/* Iconic Gemini 28px Composer */}
        <section className="gemini-composer" aria-label="PDF 转换输入区">
          <div className="composer-header">
            <div className="mode-tabs" role="tablist" aria-label="输入来源模式">
              <button
                type="button"
                role="tab"
                aria-selected={composerMode === "local"}
                className={clsx("mode-tab", composerMode === "local" && "active")}
                onClick={() => setComposerMode("local")}
              >
                <FileText size={14} />
                <span>本地文档</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={composerMode === "url"}
                className={clsx("mode-tab", composerMode === "url" && "active")}
                onClick={() => setComposerMode("url")}
              >
                <Link2 size={14} />
                <span>单条链接</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={composerMode === "batch"}
                className={clsx("mode-tab", composerMode === "batch" && "active")}
                onClick={() => setComposerMode("batch")}
              >
                <Layers size={14} />
                <span>批量链接</span>
              </button>
            </div>

            <span className="composer-counter">
              {composerMode === "local" && files.length > 0
                ? `已选 ${selectedCount} / ${files.length} 个文件`
                : composerMode === "batch" && parsedBatchUrlCount > 0
                ? `共 ${parsedBatchUrlCount} 条链接`
                : "支持 .doc · .docx · .xls · .xlsx"}
            </span>
          </div>

          <div className="composer-body">
            {composerMode === "local" ? (
              files.length === 0 ? (
                <FileDropZone onFilesAdded={addFiles} />
              ) : (
                <>
                  <FileList
                    files={files}
                    onToggle={toggleSelect}
                    onRemove={removeFile}
                    onOpenOutput={handleOpenOutputDir}
                    onPreview={(output) => setPreviewPath(output)}
                  />
                  <FileDropZone compact onFilesAdded={addFiles} />
                </>
              )
            ) : null}

            {composerMode === "url" ? (
              <>
                <input
                  type="url"
                  className="composer-input"
                  placeholder="粘贴 .doc / .docx / .xls / .xlsx 文档下载链接…"
                  value={remoteUrl}
                  onChange={(e) => setRemoteUrl(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !remoteConverting && remoteUrl.trim()) {
                      void handleConvertFromUrl();
                    }
                  }}
                />
                <div className="composer-subinput-row">
                  <input
                    type="text"
                    className="composer-input"
                    placeholder="自定义输出文件名，如 report.pdf（留空则沿用原文件名）"
                    value={remoteOutputName}
                    onChange={(e) => setRemoteOutputName(e.target.value)}
                  />
                </div>
              </>
            ) : null}

            {composerMode === "batch" ? (
              <textarea
                className="composer-textarea"
                rows={6}
                placeholder={
                  "每行粘贴一条文档下载链接（同批次请保持同为 Word 或同为 Excel）：\nhttps://example.com/a.docx\nhttps://example.com/b.docx"
                }
                value={remoteBatchUrls}
                onChange={(e) => setRemoteBatchUrls(e.target.value)}
              />
            ) : null}

            {outputEditorOpen ? (
              <div className="output-editor-bar">
                <FolderOpen size={15} />
                <input
                  type="text"
                  placeholder="留空使用默认输出目录（我的文档/PDFConverter）"
                  value={outputFolder}
                  onChange={(e) => setOutputFolder(e.target.value)}
                />
                <button
                  type="button"
                  className="pill-btn tonal compact"
                  onClick={() => void handlePickOutput()}
                >
                  浏览
                </button>
                {outputFolder ? (
                  <button
                    type="button"
                    className="pill-btn ghost compact"
                    onClick={() => setOutputFolder("")}
                  >
                    恢复默认
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>

          {/* Bottom Prompt-Style Toolbar */}
          <div className="composer-toolbar">
            <div className="composer-tools-left">
              <button
                type="button"
                className="pill-btn tonal compact"
                onClick={() => setOutputEditorOpen((prev) => !prev)}
                title={outputFolder || "使用默认输出目录（点击展开修改或选择文件夹）"}
              >
                <FolderOpen size={14} />
                <span>输出：{outputLabel}</span>
              </button>

              {outputFolder || inferredOutputFolder ? (
                <button
                  type="button"
                  className="pill-btn ghost compact"
                  onClick={() => void handleOpenFolder()}
                  title="在文件管理器中打开输出目录"
                >
                  打开目录
                </button>
              ) : null}

              {composerMode === "local" && files.length > 0 ? (
                <>
                  <button
                    type="button"
                    className="pill-btn ghost compact"
                    onClick={() => selectAll(!allSelected)}
                  >
                    {allSelected ? <Square size={14} /> : <CheckSquare size={14} />}
                    <span>{allSelected ? "取消全选" : "全选"}</span>
                  </button>

                  {files.some((f) => f.status === "failed") ? (
                    <button
                      type="button"
                      className="pill-btn ghost compact"
                      disabled={status === "processing"}
                      onClick={handleRetryFailed}
                    >
                      <RotateCcw size={14} />
                      <span>重试失败项</span>
                    </button>
                  ) : null}

                  <button
                    type="button"
                    className="pill-btn ghost compact"
                    disabled={status === "processing"}
                    onClick={handleClear}
                  >
                    <Trash2 size={14} />
                    <span>清空</span>
                  </button>
                </>
              ) : null}
            </div>

            <div className="composer-tools-right">
              {composerMode === "local" ? (
                status === "processing" ? (
                  <button
                    type="button"
                    className="pill-btn danger convert-cta"
                    onClick={() => void cancel()}
                    disabled={cancelling}
                  >
                    <Square size={14} fill="currentColor" />
                    <span>{cancelling ? "正在取消…" : "停止转换"}</span>
                  </button>
                ) : (
                  <button
                    type="button"
                    className="pill-btn convert-cta"
                    onClick={() => void startBatch(outputFolder || undefined)}
                    disabled={selectedCount === 0}
                  >
                    <ArrowUp size={17} strokeWidth={2.4} />
                    <span>开始转换{selectedCount > 0 ? ` (${selectedCount})` : ""}</span>
                  </button>
                )
              ) : null}

              {composerMode === "url" ? (
                <button
                  type="button"
                  className="pill-btn convert-cta"
                  onClick={() => void handleConvertFromUrl()}
                  disabled={remoteConverting || !remoteUrl.trim()}
                >
                  <ArrowUp size={17} strokeWidth={2.4} />
                  <span>{remoteConverting ? "下载转换中…" : "下载并转 PDF"}</span>
                </button>
              ) : null}

              {composerMode === "batch" ? (
                <button
                  type="button"
                  className="pill-btn convert-cta"
                  onClick={() => void handleBatchConvertFromUrls()}
                  disabled={remoteBatchConverting || parsedBatchUrlCount === 0}
                >
                  <ArrowUp size={17} strokeWidth={2.4} />
                  <span>
                    {remoteBatchConverting
                      ? "批量转换中…"
                      : `批量转 PDF${parsedBatchUrlCount > 0 ? ` (${parsedBatchUrlCount})` : ""}`}
                  </span>
                </button>
              ) : null}
            </div>
          </div>
        </section>

        {/* Local Mode Progress, Errors & Collapsible Log Stream */}
        {composerMode === "local" ? (
          <>
            <ProgressBar
              progress={progress}
              currentFile={currentFile}
              status={status}
              completed={completedFiles}
              total={totalFiles}
            />

            {error ? (
              <div className="response-banner error">
                <span>{error}</span>
              </div>
            ) : null}

            {logs.length > 0 ? (
              <details className="gemini-disclosure" open={status === "processing"}>
                <summary>
                  <span className="row-inline">
                    <Terminal size={15} />
                    <span>运行日志 ({logs.length})</span>
                  </span>
                  <ChevronDown size={16} />
                </summary>
                <div className="disclosure-body">
                  <LogPanel logs={logs} />
                </div>
              </details>
            ) : null}
          </>
        ) : null}

        {/* Single URL Result Stream */}
        {composerMode === "url" ? (
          <>
            {remoteResult?.success && remoteResult.output_file ? (
              <div className="response-banner success">
                <div className="row-inline">
                  <CheckCircle2 size={18} />
                  <span>转换完成：{remoteResult.output_file}</span>
                </div>
                <div className="row-inline">
                  <button
                    type="button"
                    className="pill-btn tonal compact"
                    onClick={() => setPreviewPath(remoteResult.output_file!)}
                  >
                    <Eye size={14} />
                    <span>预览 PDF</span>
                  </button>
                  <button
                    type="button"
                    className="pill-btn tonal compact"
                    onClick={() => void handleOpenOutputDir(remoteResult.output_file!)}
                  >
                    <FolderOpen size={14} />
                    <span>打开目录</span>
                  </button>
                </div>
              </div>
            ) : null}

            {remoteError ? (
              <div className="response-banner error">
                <span>{remoteError}</span>
              </div>
            ) : null}
          </>
        ) : null}

        {/* Batch URL Result Stream */}
        {composerMode === "batch" ? (
          <>
            {remoteBatchStatus ? (
              <ProgressBar
                progress={remoteBatchStatus.progress}
                currentFile={remoteBatchStatus.current_file}
                status={
                  remoteBatchStatus.status === "pending"
                    ? "processing"
                    : remoteBatchStatus.status
                }
                completed={remoteBatchStatus.completed_files}
                total={remoteBatchStatus.total_files}
              />
            ) : null}

            {remoteBatchError ? (
              <div className="response-banner error">
                <span>{remoteBatchError}</span>
              </div>
            ) : null}

            {remoteBatchStatus?.results?.length ? (
              <div className="surface-card flush">
                <table className="gemini-table">
                  <thead>
                    <tr>
                      <th>#</th>
                      <th>文件</th>
                      <th>状态</th>
                      <th>输出路径 / 原因</th>
                      <th>操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {remoteBatchStatus.results.map((item, index) => {
                      const success = item.status === "success";
                      return (
                        <tr key={`${item.file}-${index}`}>
                          <td className="mono-id">{index + 1}</td>
                          <td className="cell-break">{item.file}</td>
                          <td>
                            <span className={`status-pill ${success ? "success" : "failed"}`}>
                              {success ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                              <span>{success ? "已完成" : "失败"}</span>
                            </span>
                          </td>
                          <td className={success ? "cell-break" : "cell-error"}>
                            {success
                              ? item.output ?? "—"
                              : formatConversionError(item.error_code, item.error)}
                          </td>
                          <td>
                            <div className="row-inline">
                              {success && item.output ? (
                                <>
                                  <button
                                    type="button"
                                    className="icon-btn small"
                                    onClick={() => setPreviewPath(item.output!)}
                                    title="预览 PDF"
                                    aria-label={`预览 ${item.file}`}
                                  >
                                    <Eye size={15} />
                                  </button>
                                  <button
                                    type="button"
                                    className="icon-btn small"
                                    onClick={() => void navigator.clipboard.writeText(item.output!)}
                                    title="复制输出路径"
                                    aria-label={`复制 ${item.file} 路径`}
                                  >
                                    <Copy size={15} />
                                  </button>
                                </>
                              ) : !success ? (
                                <button
                                  type="button"
                                  className="pill-btn tonal compact"
                                  disabled={remoteBatchConverting}
                                  onClick={() => void handleRetryRemoteUrl(item.file)}
                                >
                                  <RotateCcw size={13} />
                                  <span>重试</span>
                                </button>
                              ) : null}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            ) : null}
          </>
        ) : null}
      </div>
    );
  };

  const renderHistoryView = () => {
    const filtered =
      historyFilter === "all"
        ? historyTasks
        : historyTasks.filter((t) => t.status === historyFilter);

    return (
      <div className="workspace-column wide">
        <div className="page-header">
          <div>
            <h1 className="page-title">历史记录</h1>
            <p className="page-subtitle">保留最近 100 条转换任务，应用重启后依然可查阅与导出报告</p>
          </div>
          <button
            type="button"
            className="pill-btn tonal"
            onClick={() => void refreshHistory()}
            disabled={historyLoading}
          >
            <RefreshCw size={15} className={clsx(historyLoading && "spin")} />
            <span>{historyLoading ? "刷新中…" : "刷新"}</span>
          </button>
        </div>

        <div className="filter-chips" role="tablist" aria-label="任务状态筛选">
          {[
            { key: "all", label: "全部" },
            { key: "completed", label: "已完成" },
            { key: "failed", label: "失败" },
            { key: "cancelled", label: "已取消" }
          ].map((f) => (
            <button
              key={f.key}
              type="button"
              role="tab"
              aria-selected={historyFilter === f.key}
              className={clsx("filter-chip", historyFilter === f.key && "active")}
              onClick={() => setHistoryFilter(f.key)}
            >
              {f.label}
            </button>
          ))}
        </div>

        {historyError ? (
          <div className="response-banner error">
            <span>{historyError}</span>
          </div>
        ) : null}

        {!filtered.length && !historyLoading ? (
          <div className="surface-card">
            <div className="empty-placeholder">暂无符合条件的转换记录</div>
          </div>
        ) : (
          <div className="surface-card flush">
            <table className="gemini-table">
              <thead>
                <tr>
                  <th>开始时间</th>
                  <th>状态</th>
                  <th>进度</th>
                  <th>文件 / 详情</th>
                  <th>任务 ID</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((task) => {
                  const firstSuccessOutput = task.results?.find(
                    (r) => r.status === "success" && r.output
                  )?.output;
                  const firstName = task.results?.[0]?.file ?? task.current_file ?? "—";
                  return (
                    <tr key={task.task_id}>
                      <td>{formatDate(task.started_at)}</td>
                      <td>
                        <span className={`status-pill ${task.status}`}>
                          <span>{statusTextMap[task.status] ?? task.status}</span>
                        </span>
                      </td>
                      <td>
                        {task.completed_files}/{task.total_files}
                      </td>
                      <td className={task.error_code || task.error ? "cell-error" : "cell-break"}>
                        {task.error_code || task.error
                          ? formatConversionError(task.error_code, task.error)
                          : task.total_files > 1
                          ? `${firstName} 等 ${task.total_files} 个文件`
                          : firstName}
                      </td>
                      <td className="mono-id">{task.task_id.slice(0, 8)}</td>
                      <td>
                        <div className="row-inline">
                          {firstSuccessOutput ? (
                            <button
                              type="button"
                              className="icon-btn small"
                              onClick={() => setPreviewPath(firstSuccessOutput)}
                              title="预览首个成功 PDF"
                              aria-label="预览 PDF"
                            >
                              <Eye size={15} />
                            </button>
                          ) : null}
                          <a
                            href={`http://127.0.0.1:${SERVER_PORT}/api/tasks/${task.task_id}/report.csv`}
                            target="_blank"
                            rel="noreferrer"
                            className="pill-btn ghost compact"
                            title="下载 CSV 报告"
                          >
                            <Download size={13} />
                            <span>CSV</span>
                          </a>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    );
  };

  const renderDiagnosticsView = () => (
    <div className="workspace-column wide">
      <div className="page-header">
        <div>
          <h1 className="page-title">系统诊断</h1>
          <p className="page-subtitle">实时检视本地 COM 引擎可用性、任务队列负载、存储空间与近期异常</p>
        </div>
        <button
          type="button"
          className="pill-btn tonal"
          onClick={() => void refreshDiagnostics()}
          disabled={diagnosticsLoading}
        >
          <RefreshCw size={15} className={clsx(diagnosticsLoading && "spin")} />
          <span>{diagnosticsLoading ? "检测中…" : "重新诊断"}</span>
        </button>
      </div>

      {diagnosticsError ? (
        <div className="response-banner error">
          <span>{diagnosticsError}</span>
        </div>
      ) : null}

      {queue ? (
        <div className="surface-card">
          <div className="surface-card-header">
            <h2 className="surface-card-title">任务队列统计</h2>
          </div>
          <div className="metric-row">
            {(
              [
                ["pending", "排队中"],
                ["processing", "转换中"],
                ["completed", "已完成"],
                ["failed", "失败"],
                ["cancelled", "已取消"]
              ] as const
            ).map(([key, label]) => (
              <div key={key} className="metric-tile">
                <div className="metric-tile-label">{label}</div>
                <div className="metric-tile-value">{queue.counts[key]}</div>
              </div>
            ))}
          </div>

          {queue.active.length > 0 ? (
            <table className="gemini-table">
              <thead>
                <tr>
                  <th>任务 ID</th>
                  <th>状态</th>
                  <th>进度</th>
                  <th>当前处理文件</th>
                </tr>
              </thead>
              <tbody>
                {queue.active.map((t) => (
                  <tr key={t.task_id}>
                    <td className="mono-id">{t.task_id.slice(0, 8)}</td>
                    <td>{statusTextMap[t.status] ?? t.status}</td>
                    <td>
                      {t.completed_files}/{t.total_files} ({t.progress}%)
                    </td>
                    <td className="cell-break">{t.current_file ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : null}
        </div>
      ) : null}

      {diagnostics ? (
        <>
          <div className="bento-grid">
            <div className="surface-card">
              <h2 className="surface-card-title">转换引擎探测</h2>
              <div className="kv-list">
                <div>
                  当前策略：<strong>{diagnostics.engine.configured}</strong>
                </div>
                {diagnostics.engine.probes.map((p) => (
                  <div key={p.engine}>
                    <strong>{p.engine.toUpperCase()}</strong>：
                    {p.available ? "可用" : "未就绪"}
                    {p.prog_id ? ` · ${p.prog_id}` : ""}
                    {p.version ? ` · v${p.version}` : ""}
                    {p.error ? <span className="cell-error"> ({p.error})</span> : null}
                  </div>
                ))}
              </div>
            </div>

            <div className="surface-card">
              <h2 className="surface-card-title">运行与安全</h2>
              <div className="kv-list">
                <div>
                  活跃任务：<strong>{diagnostics.tasks.active}</strong>
                </div>
                <div>
                  单文件超时：<strong>{diagnostics.conversion_timeout_secs} 秒</strong>
                </div>
                <div>
                  API Token 鉴权：
                  <strong>{diagnostics.security.api_token_enabled ? "已开启" : "未开启"}</strong>
                </div>
                <div>
                  应用版本：<strong>v{diagnostics.version}</strong>
                </div>
              </div>
            </div>

            <div className="surface-card">
              <h2 className="surface-card-title">存储空间</h2>
              <div className="kv-list">
                <div className="cell-break">
                  默认输出目录：<strong>{diagnostics.storage.default_output_dir ?? "未指定"}</strong>
                </div>
                <div className="cell-break">
                  回退目录：<strong>{diagnostics.storage.fallback_output_dir}</strong>
                </div>
                <div>
                  磁盘可用空间：<strong>{formatBytes(diagnostics.storage.free_bytes)}</strong>
                </div>
              </div>
            </div>
          </div>

          <div className="surface-card flush">
            <div className="setting-row">
              <div>
                <div className="surface-card-title">最近失败记录</div>
                <div className="surface-card-desc">展示最近 10 条异常明细，协助定位文档加密或引擎占用</div>
              </div>
            </div>
            {diagnostics.tasks.recent_failures.length === 0 ? (
              <div className="empty-placeholder">近期运行平稳，暂无失败记录</div>
            ) : (
              <table className="gemini-table">
                <thead>
                  <tr>
                    <th>发生时间</th>
                    <th>文件</th>
                    <th>错误原因</th>
                  </tr>
                </thead>
                <tbody>
                  {diagnostics.tasks.recent_failures.map((f, i) => (
                    <tr key={`${f.task_id}-${i}`}>
                      <td>{formatDate(f.started_at)}</td>
                      <td className="cell-break">{f.file}</td>
                      <td className="cell-error">
                        {formatConversionError(f.error_code, f.error)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </>
      ) : null}
    </div>
  );

  const renderSettingsView = () => (
    <div className="workspace-column">
      <div className="page-header">
        <div>
          <h1 className="page-title">设置</h1>
          <p className="page-subtitle">个性化界面外观、PDF 转换引擎、自动化文件夹监听与安全选项</p>
        </div>
      </div>

      {/* 外观主题 */}
      <section className="surface-card flush" aria-label="外观设置">
        <div className="setting-row">
          <div>
            <div className="setting-label">界面外观</div>
            <div className="setting-desc">
              选择浅色或深色主题，或跟随操作系统自动切换（当前系统为{systemDark ? "深色" : "浅色"}）
            </div>
          </div>
          <div className="segmented-pills" role="radiogroup" aria-label="界面主题">
            <button
              type="button"
              role="radio"
              aria-checked={themePref === "system"}
              className={clsx("segmented-pill", themePref === "system" && "active")}
              onClick={() => handleSelectTheme("system")}
            >
              <Monitor size={14} />
              <span>跟随系统</span>
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={themePref === "light"}
              className={clsx("segmented-pill", themePref === "light" && "active")}
              onClick={() => handleSelectTheme("light")}
            >
              <Sun size={14} />
              <span>浅色</span>
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={themePref === "dark"}
              className={clsx("segmented-pill", themePref === "dark" && "active")}
              onClick={() => handleSelectTheme("dark")}
            >
              <Moon size={14} />
              <span>深色</span>
            </button>
          </div>
        </div>
      </section>

      {/* 转换引擎选择 */}
      <section className="surface-card" aria-label="转换引擎">
        <div className="surface-card-header">
          <div>
            <h2 className="surface-card-title">PDF 转换引擎</h2>
            <p className="surface-card-desc">选择本机已安装的兼容套件执行高精度排版转换</p>
          </div>
          <button type="button" className="pill-btn tonal compact" onClick={handleRefresh}>
            <RefreshCw size={14} />
            <span>重新探测</span>
          </button>
        </div>

        <div className="engine-cards">
          {engineOptions.map((opt) => {
            const isCurrent = currentEngine === opt.key;
            const disabled = !opt.available || desktopAction === `engine-${opt.key}`;
            return (
              <div
                key={opt.key}
                role="button"
                tabIndex={disabled && !isCurrent ? -1 : 0}
                aria-pressed={isCurrent}
                className={clsx(
                  "engine-card",
                  isCurrent && "selected",
                  !opt.available && "unavailable"
                )}
                onClick={() => {
                  if (!disabled && !isCurrent) void handleSelectEngine(opt.key);
                }}
                onKeyDown={(e) => {
                  if (
                    (e.key === "Enter" || e.key === " ") &&
                    !disabled &&
                    !isCurrent
                  ) {
                    e.preventDefault();
                    void handleSelectEngine(opt.key);
                  }
                }}
              >
                <div className="engine-card-top">
                  <span className="engine-card-title">{opt.title}</span>
                  <span className={`status-pill ${opt.available ? "success" : "failed"}`}>
                    <span>{opt.available ? "可用" : "未安装"}</span>
                  </span>
                </div>
                <div className="engine-card-desc">{opt.desc}</div>
                {opt.available && opt.key !== "auto" ? (
                  <div className="engine-card-version">
                    {opt.version ? (
                      `v${opt.version}`
                    ) : (
                      <button
                        type="button"
                        className="text-action-btn"
                        disabled={engineVersionLoading === opt.key}
                        onClick={(e) => {
                          e.stopPropagation();
                          void handleFetchEngineVersion(opt.key);
                        }}
                      >
                        {engineVersionLoading === opt.key ? "读取中…" : "查看版本号"}
                      </button>
                    )}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </section>

      {/* 启动与托盘 */}
      <section className="surface-card flush" aria-label="启动与系统托盘">
        <div className="settings-rows">
          <label className="setting-row">
            <div>
              <div className="setting-label">开机自动启动</div>
              <div className="setting-desc">随当前 Windows 账户登录自动启动本地转换服务</div>
            </div>
            <span className="switch">
              <input
                type="checkbox"
                checked={desktopSettings?.launch_on_startup ?? false}
                onChange={(e) => void handleToggleLaunchOnStartup(e.target.checked)}
                disabled={desktopAction === "launch" || !desktopSettings}
              />
              <span className="switch-track" />
            </span>
          </label>

          <label className="setting-row">
            <div>
              <div className="setting-label">自启时静默驻留托盘</div>
              <div className="setting-desc">开机自启时不弹出主界面，直接在后台等待调用</div>
            </div>
            <span className="switch">
              <input
                type="checkbox"
                checked={desktopSettings?.silent_start_on_login ?? true}
                onChange={(e) => void handleToggleSilentStart(e.target.checked)}
                disabled={desktopAction === "silentStart" || !desktopSettings}
              />
              <span className="switch-track" />
            </span>
          </label>

          <label className="setting-row">
            <div>
              <div className="setting-label">关闭窗口最小化到托盘</div>
              <div className="setting-desc">点击窗口右上角关闭时保持后台服务运行而不退出</div>
            </div>
            <span className="switch">
              <input
                type="checkbox"
                checked={desktopSettings?.close_to_tray ?? true}
                onChange={(e) => void handleToggleCloseToTray(e.target.checked)}
                disabled={desktopAction === "closeToTray" || !desktopSettings}
              />
              <span className="switch-track" />
            </span>
          </label>
        </div>
      </section>

      {/* 转换规则与目录自动监听 */}
      <section className="surface-card flush" aria-label="转换策略与监听">
        <div className="settings-rows">
          <div className="setting-row">
            <div>
              <div className="setting-label">单文件转换超时（秒）</div>
              <div className="setting-desc">
                {serverSettings
                  ? `允许范围 ${serverSettings.conversion_timeout_min_secs} - ${serverSettings.conversion_timeout_max_secs} 秒，防止异常文档阻塞队列`
                  : "正在读取配置…"}
              </div>
              {timeoutError ? <div className="cell-error">{timeoutError}</div> : null}
            </div>
            <div className="row-inline">
              <input
                type="number"
                className="field-input"
                aria-label="单文件转换超时秒数"
                value={timeoutInput}
                min={serverSettings?.conversion_timeout_min_secs}
                max={serverSettings?.conversion_timeout_max_secs}
                onChange={(e) => setTimeoutInput(e.target.value)}
                disabled={!serverSettings || timeoutSaving}
              />
              <button
                type="button"
                className="pill-btn tonal compact"
                onClick={() => void handleSaveTimeout()}
                disabled={
                  !serverSettings ||
                  timeoutSaving ||
                  timeoutInput === String(serverSettings.conversion_timeout_secs)
                }
              >
                {timeoutSaving ? "保存中…" : "保存"}
              </button>
            </div>
          </div>

          <div className="setting-row">
            <div>
              <div className="setting-label">输出同名 PDF 处理策略</div>
              <div className="setting-desc">当目标目录已存在同名 PDF 文件时的默认行为</div>
            </div>
            <div className="segmented-pills" role="radiogroup" aria-label="同名文件策略">
              {(
                [
                  ["auto_number", "自动编号"],
                  ["overwrite", "覆盖原文件"],
                  ["skip", "跳过"]
                ] as const
              ).map(([strategy, label]) => (
                <button
                  key={strategy}
                  type="button"
                  role="radio"
                  aria-checked={serverSettings?.output_collision_strategy === strategy}
                  className={clsx(
                    "segmented-pill",
                    serverSettings?.output_collision_strategy === strategy && "active"
                  )}
                  disabled={!serverSettings}
                  onClick={async () => {
                    if (!serverSettings) return;
                    try {
                      const result = await updateSettings({ output_collision_strategy: strategy });
                      setServerSettings(result);
                    } catch {
                      window.alert("保存策略失败");
                    }
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="setting-row stacked">
            <div className="surface-card-header">
              <div>
                <div className="setting-label">文件夹自动监听转换</div>
                <div className="setting-desc">
                  放入源目录的 Word / Excel 文档停止写入约 5 秒后，将自动转换为 PDF 输出到目标目录
                </div>
              </div>
              <button
                type="button"
                className="pill-btn tonal compact"
                disabled={!serverSettings}
                onClick={async () => {
                  if (!serverSettings) return;
                  const next = serverSettings.watch_folders.concat({
                    src: "",
                    dst: "",
                    delete_source: false
                  });
                  const result = await updateSettings({ watch_folders: next });
                  setServerSettings(result);
                }}
              >
                <Plus size={14} />
                <span>添加目录对</span>
              </button>
            </div>

            {(serverSettings?.watch_folders ?? []).map((folder, idx) => (
              <div key={idx} className="watch-folder-row">
                <input
                  type="text"
                  className="field-input"
                  placeholder="监听源目录路径"
                  value={folder.src}
                  onChange={async (e) => {
                    if (!serverSettings) return;
                    const next = serverSettings.watch_folders.slice();
                    next[idx] = { ...folder, src: e.target.value };
                    const result = await updateSettings({ watch_folders: next });
                    setServerSettings(result);
                  }}
                />
                <button
                  type="button"
                  className="pill-btn ghost compact"
                  onClick={() =>
                    void handlePickFolderInto(async (picked) => {
                      if (!serverSettings) return;
                      const next = serverSettings.watch_folders.slice();
                      next[idx] = { ...folder, src: picked };
                      const result = await updateSettings({ watch_folders: next });
                      setServerSettings(result);
                    })
                  }
                >
                  选源目录
                </button>
                <input
                  type="text"
                  className="field-input"
                  placeholder="PDF 输出目录路径"
                  value={folder.dst}
                  onChange={async (e) => {
                    if (!serverSettings) return;
                    const next = serverSettings.watch_folders.slice();
                    next[idx] = { ...folder, dst: e.target.value };
                    const result = await updateSettings({ watch_folders: next });
                    setServerSettings(result);
                  }}
                />
                <button
                  type="button"
                  className="pill-btn ghost compact"
                  onClick={() =>
                    void handlePickFolderInto(async (picked) => {
                      if (!serverSettings) return;
                      const next = serverSettings.watch_folders.slice();
                      next[idx] = { ...folder, dst: picked };
                      const result = await updateSettings({ watch_folders: next });
                      setServerSettings(result);
                    })
                  }
                >
                  选输出目录
                </button>
                <label className="row-inline">
                  <input
                    type="checkbox"
                    className="checkbox"
                    checked={folder.delete_source}
                    onChange={async (e) => {
                      if (!serverSettings) return;
                      const next = serverSettings.watch_folders.slice();
                      next[idx] = { ...folder, delete_source: e.target.checked };
                      const result = await updateSettings({ watch_folders: next });
                      setServerSettings(result);
                    }}
                  />
                  <span>删除源文件</span>
                </label>
                <button
                  type="button"
                  className="icon-btn small"
                  title="移除该监听规则"
                  aria-label="移除该监听规则"
                  onClick={async () => {
                    if (!serverSettings) return;
                    const next = serverSettings.watch_folders.filter((_, i) => i !== idx);
                    const result = await updateSettings({ watch_folders: next });
                    setServerSettings(result);
                  }}
                >
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* 安全与服务控制 */}
      <section className="surface-card flush" aria-label="安全与服务维护">
        <div className="settings-rows">
          <div className="setting-row">
            <div>
              <div className="setting-label">
                <span className="row-inline">
                  <Shield size={15} />
                  <span>本地 HTTP API Token 鉴权</span>
                </span>
              </div>
              <div className="setting-desc">
                启用后外部调用 <code>/api/*</code> 需携带 <code>Authorization: Bearer &lt;token&gt;</code>，桌面界面会自动注入
              </div>
              {tokenInfo?.enabled && tokenInfo.token ? (
                <div className="token-box">{tokenInfo.token}</div>
              ) : null}
            </div>

            <div className="row-inline">
              {tokenInfo?.enabled && tokenInfo.token ? (
                <>
                  <button
                    type="button"
                    className="pill-btn tonal compact"
                    onClick={() => void handleCopyApiToken()}
                    disabled={tokenAction !== null}
                  >
                    {tokenCopied ? <Check size={14} /> : <Copy size={14} />}
                    <span>{tokenCopied ? "已复制" : "复制"}</span>
                  </button>
                  <button
                    type="button"
                    className="pill-btn tonal compact"
                    onClick={() => void handleRegenerateApiToken()}
                    disabled={tokenAction !== null}
                  >
                    <RefreshCw size={14} />
                    <span>{tokenAction === "regenerate" ? "生成中…" : "重置"}</span>
                  </button>
                </>
              ) : null}
              <span className="switch">
                <input
                  type="checkbox"
                  aria-label="启用本地 API Token"
                  checked={tokenInfo?.enabled ?? false}
                  onChange={(e) => void handleToggleApiToken(e.target.checked)}
                  disabled={tokenAction !== null || !tokenInfo}
                />
                <span className="switch-track" />
              </span>
            </div>
          </div>

          <div className="setting-row">
            <div>
              <div className="setting-label">服务维护与更新</div>
              <div className="setting-desc">调试本地 REST 接口文档、驻留系统托盘或检查客户端新版本</div>
            </div>
            <div className="row-inline">
              <button type="button" className="pill-btn tonal compact" onClick={handleRefresh}>
                <RefreshCw size={14} />
                <span>刷新状态</span>
              </button>
              <button
                type="button"
                className="pill-btn tonal compact"
                onClick={() => void openShell(`http://127.0.0.1:${SERVER_PORT}/`)}
              >
                <Globe size={14} />
                <span>Web 控制台</span>
              </button>
              <button type="button" className="pill-btn tonal compact" onClick={handleHideToTray}>
                <EyeOff size={14} />
                <span>隐藏到托盘</span>
              </button>
              <button
                type="button"
                className="pill-btn compact"
                onClick={() => void handleCheckUpdate()}
                disabled={updateChecking}
              >
                <RefreshCw size={14} className={clsx(updateChecking && "spin")} />
                <span>{updateChecking ? "检查中…" : "检查更新"}</span>
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>
  );

  return (
    <div className={clsx("gemini-shell", sidebarCollapsed && "sidebar-collapsed")}>
      <NavigationRail
        active={activeNav}
        mode={composerMode}
        collapsed={sidebarCollapsed}
        onToggleCollapse={handleToggleSidebar}
        onSelectWorkspace={handleSelectWorkspaceMode}
        onNewConversion={handleNewConversion}
        onChangeNav={setActiveNav}
        recentTasks={historyTasks}
        serverOk={!!health?.status}
        port={SERVER_PORT}
      />

      <div className="gemini-main">
        {/* Gemini Top Canvas Header with Model/Engine Switcher */}
        <header className="canvas-topbar">
          <div className="topbar-brand-area">
            <details ref={engineDropdownRef} className="engine-switcher">
              <summary>
                <AppIcon size={19} />
                <span>AlexPDF</span>
                <span className="engine-switcher-tag">
                  {engineLabels[currentEngine]}
                  {!engineAvailable ? " · 未就绪" : ""}
                </span>
                <ChevronDown size={15} />
              </summary>

              <div className="engine-dropdown-card">
                <div className="engine-dropdown-label">切换 PDF 转换内核</div>
                {engineOptions.map((opt) => {
                  const isCurrent = currentEngine === opt.key;
                  return (
                    <button
                      key={opt.key}
                      type="button"
                      className={clsx("engine-dropdown-item", isCurrent && "selected")}
                      disabled={!opt.available && !isCurrent}
                      onClick={() => void handleSelectEngine(opt.key)}
                    >
                      <div>
                        <div className="engine-dropdown-item-title">{opt.title}</div>
                        <div className="engine-dropdown-item-sub">
                          {opt.available ? "本机已就绪" : "未检测到安装"}
                        </div>
                      </div>
                      {isCurrent ? <Check size={16} /> : null}
                    </button>
                  );
                })}
              </div>
            </details>
          </div>

          <StatusBar
            serverOk={!!health?.status}
            version={health?.version}
            onVersionClick={handleVersionChipClick}
            versionChecking={updateChecking}
            updateAvailable={
              !!updateInfo?.update_available &&
              !!updateInfo.latest_version &&
              updateInfo.latest_version !== skippedVersion
            }
            latestVersion={updateInfo?.latest_version}
            onOpenWebConsole={() => void openShell(`http://127.0.0.1:${SERVER_PORT}/`)}
            onHideToTray={() => void handleHideToTray()}
          />
        </header>

        <main className="canvas-scroll">
          {activeNav === "history"
            ? renderHistoryView()
            : activeNav === "diagnostics"
            ? renderDiagnosticsView()
            : activeNav === "settings"
            ? renderSettingsView()
            : renderWorkspaceView()}
        </main>
      </div>

      {/* Native <dialog>: PDF Preview */}
      <ModalDialog
        open={!!previewPath}
        onClose={() => setPreviewPath(null)}
        title="PDF 文档预览"
        subtitle={previewPath ?? undefined}
        large
        closedBy="any"
        actions={
          <>
            {previewPath ? (
              <button
                type="button"
                className="pill-btn tonal"
                onClick={() => void handleOpenOutputDir(previewPath)}
              >
                <FolderOpen size={15} />
                <span>打开所在目录</span>
              </button>
            ) : null}
            <button type="button" className="pill-btn" onClick={() => setPreviewPath(null)}>
              关闭
            </button>
          </>
        }
      >
        {previewPath ? (
          <iframe
            title="PDF 预览"
            src={`http://127.0.0.1:${SERVER_PORT}/api/preview?path=${encodeURIComponent(
              previewPath
            )}`}
          />
        ) : null}
      </ModalDialog>

      {/* Native <dialog>: App Update */}
      <ModalDialog
        open={updateVisible && !!updateInfo}
        onClose={() => setUpdateVisible(false)}
        title="发现新版本"
        subtitle={
          updateInfo
            ? `当前 v${updateInfo.current_version}${
                updateInfo.latest_version ? ` → 最新 v${updateInfo.latest_version}` : ""
              }`
            : undefined
        }
        closedBy={
          updateProgress?.status === "downloading" || updateProgress?.status === "verifying"
            ? "closerequest"
            : "any"
        }
        actions={
          updateProgress?.status === "ready" ? (
            <>
              <button
                type="button"
                className="pill-btn ghost"
                onClick={() => setUpdateVisible(false)}
                disabled={updateInstalling}
              >
                稍后安装
              </button>
              <button
                type="button"
                className="pill-btn"
                onClick={() => void handleLaunchUpdate()}
                disabled={updateInstalling}
              >
                {updateInstalling ? "启动中…" : "立即重启并安装"}
              </button>
            </>
          ) : updateProgress?.status === "downloading" ||
            updateProgress?.status === "verifying" ? (
            <>
              <button
                type="button"
                className="pill-btn danger"
                onClick={() => void handleCancelUpdate()}
                disabled={updateCancelling}
              >
                {updateCancelling ? "正在取消…" : "取消下载"}
              </button>
              <button
                type="button"
                className="pill-btn tonal"
                onClick={() => setUpdateVisible(false)}
              >
                后台继续下载
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                className="pill-btn ghost"
                onClick={handleSkipVersion}
                disabled={updateInstalling}
              >
                跳过此版本
              </button>
              <button
                type="button"
                className="pill-btn tonal"
                onClick={() => setUpdateVisible(false)}
                disabled={updateInstalling}
              >
                稍后提醒
              </button>
              <button
                type="button"
                className="pill-btn"
                onClick={() => void handleInstallUpdate()}
                disabled={updateInstalling}
              >
                {updateInstalling
                  ? "下载中…"
                  : updateProgress?.status === "cancelled"
                  ? "继续下载"
                  : updateProgress?.status === "failed"
                  ? "重新下载"
                  : "下载并安装"}
              </button>
            </>
          )
        }
      >
        {updateInfo ? (
          <div className="kv-list">
            <div className="dialog-notes">{updateInfo.notes || "本次版本更新包含稳定性与体验优化。"}</div>
            {updateProgress &&
            (updateProgress.status === "downloading" ||
              updateProgress.status === "verifying" ||
              updateProgress.status === "ready" ||
              updateProgress.status === "cancelled" ||
              updateProgress.status === "failed") ? (
              <div className="gemini-progress-card">
                <div className="progress-header">
                  <span className="progress-label">
                    {updateProgress.status === "downloading" &&
                      (updateProgress.is_resume ? "断点续传中" : "正在下载更新包")}
                    {updateProgress.status === "verifying" && "正在校验完整性"}
                    {updateProgress.status === "ready" && "安装包已就绪"}
                    {updateProgress.status === "cancelled" && "已暂停下载"}
                    {updateProgress.status === "failed" && "下载失败"}
                  </span>
                  <span className="progress-meta">
                    {updateProgress.total > 0
                      ? `${formatMB(updateProgress.downloaded)} / ${formatMB(updateProgress.total)}`
                      : formatMB(updateProgress.downloaded)}
                  </span>
                </div>
                <div className="progress-track">
                  <div
                    className={clsx(
                      "progress-bar",
                      updateProgress.status === "downloading" && "shimmer",
                      updateProgress.status === "ready" && "complete",
                      updateProgress.status === "failed" && "error"
                    )}
                    style={{
                      width:
                        updateProgress.total > 0
                          ? `${Math.min(
                              100,
                              (updateProgress.downloaded / updateProgress.total) * 100
                            )}%`
                          : "0%"
                    }}
                  />
                </div>
                {(updateProgress.status === "failed" ||
                  updateProgress.status === "cancelled") &&
                updateProgress.error ? (
                  <div className="cell-error">{updateProgress.error}</div>
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}
      </ModalDialog>

      {/* Native <dialog>: Pending Install Failure Recovery */}
      <ModalDialog
        open={pendingInstallVisible && !!pendingInstall?.failed && !!pendingInstall.pending}
        onClose={() => void handleAcknowledgePendingFailure()}
        title="上次更新安装未完成"
        subtitle={
          pendingInstall ? `当前仍运行 v${pendingInstall.current_version}` : undefined
        }
        closedBy="closerequest"
        actions={
          <>
            <button
              type="button"
              className="pill-btn ghost"
              onClick={() => void handleAcknowledgePendingFailure()}
            >
              忽略
            </button>
            {pendingInstall?.pending ? (
              <button
                type="button"
                className="pill-btn tonal"
                onClick={() => {
                  const targetPath = pendingInstall.pending!.installer_path;
                  void handleAcknowledgePendingFailure();
                  void revealInExplorer(targetPath);
                }}
              >
                <FolderOpen size={15} />
                <span>定位安装包</span>
              </button>
            ) : null}
            <button
              type="button"
              className="pill-btn"
              onClick={() => {
                setPendingInstallVisible(false);
                void handleCheckUpdate();
              }}
            >
              重新检查更新
            </button>
          </>
        }
      >
        <div className="dialog-notes">
          上次启动安装程序后版本号未发生变化，可能是取消了 UAC 授权或安装中途被安全软件拦截。
          {pendingInstall?.pending
            ? `\n\n已下载安装包路径：\n${pendingInstall.pending.installer_path}`
            : ""}
        </div>
      </ModalDialog>
    </div>
  );
}

export default App;
