import clsx from "clsx";
import { AppIcon } from "./AppIcon";

type ProgressBarProps = {
  progress: number;
  currentFile?: string | null;
  status: "idle" | "processing" | "completed" | "failed" | "cancelled";
  completed: number;
  total: number;
};

export function ProgressBar({
  progress,
  currentFile,
  status,
  completed,
  total
}: ProgressBarProps) {
  if (status === "idle" && total === 0) {
    return null;
  }

  const label =
    status === "processing"
      ? `正在转换为 PDF (${completed}/${total})`
      : status === "completed"
      ? `转换已完成 · 共 ${total} 个文档`
      : status === "failed"
      ? "部分或全部文档转换失败"
      : status === "cancelled"
      ? "转换任务已取消"
      : `已就绪 · 待转换 ${total} 个文档`;

  const percent = Math.max(0, Math.min(100, Math.round(progress)));

  return (
    <div className="gemini-progress-card">
      <div className="progress-header">
        <span className="progress-label">
          {status === "processing" ? <AppIcon size={16} animated /> : null}
          <span>{label}</span>
        </span>
        <span className="progress-meta">{percent}%</span>
      </div>
      <div className="progress-track">
        <div
          className={clsx(
            "progress-bar",
            status === "processing" && "shimmer",
            status === "completed" && "complete",
            (status === "failed" || status === "cancelled") && "error"
          )}
          style={{ width: `${percent}%` }}
        />
      </div>
      {currentFile ? <div className="progress-current">正在处理：{currentFile}</div> : null}
    </div>
  );
}
