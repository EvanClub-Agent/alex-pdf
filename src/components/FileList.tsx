import { CheckCircle2, Clock, Eye, FolderOpen, Loader2, X, XCircle } from "lucide-react";
import clsx from "clsx";
import type { FileItem } from "../types";
import { formatConversionError } from "../types";

const statusLabel: Record<FileItem["status"], string> = {
  ready: "待转换",
  processing: "转换中",
  success: "已完成",
  failed: "失败"
};

function formatBytes(bytes: number) {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const value = bytes / Math.pow(k, i);
  return `${value.toFixed(1)} ${sizes[i]}`;
}

function getExtension(name: string) {
  const dot = name.lastIndexOf(".");
  if (dot < 0) return "";
  return name.slice(dot + 1).toLowerCase();
}

function fileIconClass(ext: string) {
  if (ext === "doc" || ext === "docx") return "word";
  if (ext === "xls" || ext === "xlsx") return "excel";
  return "generic";
}

function fileIconLabel(ext: string) {
  if (ext === "doc" || ext === "docx") return "W";
  if (ext === "xls" || ext === "xlsx") return "X";
  return ext.slice(0, 1).toUpperCase() || "?";
}

function StatusIcon({ status }: { status: FileItem["status"] }) {
  if (status === "processing") return <Loader2 size={13} className="spin" aria-hidden="true" />;
  if (status === "success") return <CheckCircle2 size={13} aria-hidden="true" />;
  if (status === "failed") return <XCircle size={13} aria-hidden="true" />;
  return <Clock size={13} aria-hidden="true" />;
}

export function FileList({
  files,
  onToggle,
  onRemove,
  onOpenOutput,
  onPreview
}: {
  files: FileItem[];
  onToggle: (id: string) => void;
  onRemove: (id: string) => void;
  onOpenOutput?: (output: string) => void;
  onPreview?: (output: string) => void;
}) {
  if (!files.length) {
    return null;
  }

  return (
    <div className="attachment-list" role="list" aria-label="待转换文件列表">
      {files.map((file) => {
        const ext = getExtension(file.name);
        const hasOutput = file.status === "success" && !!file.output;
        return (
          <div
            role="listitem"
            className={clsx("attachment-card", file.selected && "selected")}
            key={file.id}
          >
            <label className="checkbox-wrap" aria-label={`选择 ${file.name}`}>
              <input
                className="checkbox"
                type="checkbox"
                checked={file.selected}
                onChange={() => onToggle(file.id)}
              />
              <span className="checkbox-fallback" aria-hidden="true" />
            </label>

            <span className={`file-badge-icon ${fileIconClass(ext)}`} aria-hidden="true">
              {fileIconLabel(ext)}
            </span>

            <div className="attachment-info">
              <div className="attachment-name-row">
                <span className="attachment-name" title={file.name}>
                  {file.name}
                </span>
              </div>
              <div className="attachment-meta">
                <span>{formatBytes(file.size)}</span>
                {file.durationMs ? <span>· {file.durationMs} ms</span> : null}
                {file.output ? (
                  <>
                    <span>·</span>
                    <button
                      type="button"
                      className="link-text"
                      onClick={() => hasOutput && onOpenOutput?.(file.output!)}
                      disabled={!hasOutput}
                      title={file.output}
                    >
                      {file.output}
                    </button>
                  </>
                ) : null}
                {file.error || file.errorCode ? (
                  <span className="attachment-error">
                    · {formatConversionError(file.errorCode, file.error)}
                  </span>
                ) : null}
              </div>
            </div>

            <span className={`status-pill ${file.status}`}>
              <StatusIcon status={file.status} />
              <span>{statusLabel[file.status]}</span>
            </span>

            <div className="attachment-actions">
              {hasOutput ? (
                <>
                  <button
                    type="button"
                    className="icon-btn small"
                    onClick={() => onPreview?.(file.output!)}
                    title="预览 PDF"
                    aria-label={`预览 ${file.name} 的 PDF`}
                  >
                    <Eye size={15} />
                  </button>
                  <button
                    type="button"
                    className="icon-btn small"
                    onClick={() => onOpenOutput?.(file.output!)}
                    title="在文件管理器中定位"
                    aria-label={`打开 ${file.name} 所在目录`}
                  >
                    <FolderOpen size={15} />
                  </button>
                </>
              ) : null}
              <button
                type="button"
                className="icon-btn small subtle"
                onClick={() => onRemove(file.id)}
                title="移除文件"
                aria-label={`移除 ${file.name}`}
              >
                <X size={15} />
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
