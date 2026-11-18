import { useCallback, useEffect, useRef, useState } from "react";

import { cancelTask, convertBatch, getStatus } from "../services/api";
import type { FileItem, FileResult, TaskStatusResponse } from "../types";

const POLL_INTERVAL = 900;

function mapResults(results: FileResult[] | null | undefined) {
  const map = new Map<string, FileResult>();
  results?.forEach((item) => map.set(item.file, item));
  return map;
}

export function useConversion(
  files: FileItem[],
  updateFiles: (updater: (prev: FileItem[]) => FileItem[]) => void
) {
  const [taskId, setTaskId] = useState<string | null>(null);
  type UiStatus = "idle" | "processing" | "completed" | "failed" | "cancelled";
  const [status, setStatus] = useState<UiStatus>("idle");
  const [cancelling, setCancelling] = useState(false);
  const [progress, setProgress] = useState(0);
  const [currentFile, setCurrentFile] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [totalFiles, setTotalFiles] = useState(0);
  const [completedFiles, setCompletedFiles] = useState(0);

  const activeNames = useRef<Set<string>>(new Set());
  const activeIds = useRef<Set<string>>(new Set());

  const addLog = useCallback((message: string) => {
    setLogs((prev) => [`${new Date().toLocaleTimeString()} ${message}`, ...prev]);
  }, []);

  const reset = useCallback(() => {
    setTaskId(null);
    setStatus("idle");
    setProgress(0);
    setCurrentFile(null);
    setError(null);
    setTotalFiles(0);
    setCompletedFiles(0);
    setCancelling(false);
  }, []);

  const startBatch = useCallback(
    async (outputFolder?: string) => {
      const selected = files.filter((item) => item.selected);
      if (!selected.length) {
        setError("请先选择要转换的文件");
        return;
      }

      setStatus("processing");
      setProgress(0);
      setCurrentFile(null);
      setError(null);
      setTotalFiles(selected.length);
      setCompletedFiles(0);

      activeIds.current = new Set(selected.map((item) => item.id));
      activeNames.current = new Set(selected.map((item) => item.name));

      updateFiles((prev) =>
        prev.map((item) =>
          activeIds.current.has(item.id)
            ? { ...item, status: "processing", error: undefined }
            : item
        )
      );

      try {
        addLog(`开始转换 ${selected.length} 个文件`);
        const response = await convertBatch(
          selected.map((item) => item.file),
          outputFolder,
          true
        );

        if (!response.task_id) {
          throw new Error("未能获取任务 ID");
        }

        setTaskId(response.task_id);
        addLog("任务已提交，等待处理...");
      } catch (err) {
        const message = err instanceof Error ? err.message : "启动转换失败";
        setError(message);
        setStatus("failed");
        addLog(`错误: ${message}`);
      }
    },
    [addLog, files, updateFiles]
  );

  useEffect(() => {
    if (!taskId) {
      return;
    }

    let active = true;
    const interval = setInterval(async () => {
      try {
        const statusResponse: TaskStatusResponse = await getStatus(taskId);
        if (!active) {
          return;
        }

        setProgress(statusResponse.progress);
        setCurrentFile(statusResponse.current_file ?? null);
        const nextStatus: UiStatus =
          statusResponse.status === "pending" ? "processing" : (statusResponse.status as UiStatus);
        setStatus(nextStatus);
        setTotalFiles(statusResponse.total_files);
        setCompletedFiles(statusResponse.completed_files);

        if (
          statusResponse.status === "completed" ||
          statusResponse.status === "failed" ||
          statusResponse.status === "cancelled"
        ) {
          const resultMap = mapResults(statusResponse.results);
          updateFiles((prev) =>
            prev.map((item) => {
              if (!activeNames.current.has(item.name)) {
                return item;
              }
              const result = resultMap.get(item.name);
              if (!result) {
                return {
                  ...item,
                  status: statusResponse.status === "completed" ? "success" : "failed",
                  error: statusResponse.error ?? item.error,
                  errorCode: statusResponse.error_code ?? item.errorCode
                };
              }
              return {
                ...item,
                status: result.status === "success" ? "success" : "failed",
                output: result.output ?? undefined,
                error: result.error ?? undefined,
                errorCode: result.error_code ?? undefined,
                durationMs: result.duration_ms ?? undefined,
                selected: false
              };
            })
          );

          const finishLog =
            statusResponse.status === "completed"
              ? "转换完成"
              : statusResponse.status === "cancelled"
                ? "任务已取消"
                : "转换失败";
          addLog(finishLog);
          clearInterval(interval);
          setTaskId(null);
          setCancelling(false);
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : "状态查询失败";
        setError(message);
        setStatus("failed");
        addLog(`错误: ${message}`);
        clearInterval(interval);
        setTaskId(null);
      }
    }, POLL_INTERVAL);

    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [addLog, taskId, updateFiles]);

  const cancel = useCallback(async () => {
    if (!taskId) return;
    setCancelling(true);
    try {
      await cancelTask(taskId);
      addLog("已请求取消...");
    } catch (err) {
      const message = err instanceof Error ? err.message : "取消失败";
      addLog(`错误: ${message}`);
      setCancelling(false);
    }
  }, [addLog, taskId]);

  return {
    taskId,
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
  };
}
