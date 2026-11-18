import { useState } from "react";
import clsx from "clsx";
import { FileUp, Plus } from "lucide-react";
import { useDropzone, type FileRejection } from "react-dropzone";

type FileDropZoneProps = {
  onFilesAdded: (files: File[]) => void;
  compact?: boolean;
  title?: string;
  hint?: string;
};

const OFFICE_ACCEPT = {
  "application/msword": [".doc"],
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document": [".docx"],
  "application/vnd.ms-excel": [".xls"],
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet": [".xlsx"]
} as const;

const OFFICE_EXTENSIONS = [".doc", ".docx", ".xls", ".xlsx"];

export function FileDropZone({
  onFilesAdded,
  compact = false,
  title,
  hint
}: FileDropZoneProps) {
  const [rejectedNotice, setRejectedNotice] = useState<string | null>(null);

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
    multiple: true,
    accept: OFFICE_ACCEPT,
    onDrop: (acceptedFiles, rejected: FileRejection[]) => {
      if (acceptedFiles.length) {
        onFilesAdded(acceptedFiles);
      }
      if (rejected.length) {
        const names = rejected
          .map((item) => item.file.name)
          .slice(0, 3)
          .join("、");
        const more = rejected.length > 3 ? ` 等 ${rejected.length} 个` : "";
        setRejectedNotice(`已忽略不支持的文件: ${names}${more}`);
        window.setTimeout(() => setRejectedNotice(null), 4000);
      } else {
        setRejectedNotice(null);
      }
    }
  });

  if (compact) {
    return (
      <div
        {...getRootProps()}
        className={clsx("dropzone-compact", isDragActive && "active")}
      >
        <input {...getInputProps()} />
        <Plus size={16} />
        <span>
          {isDragActive
            ? "松开即可加入待转换列表"
            : "继续拖入或点击添加 Word / Excel 文档"}
        </span>
        {rejectedNotice ? <span className="dropzone-error-inline">{rejectedNotice}</span> : null}
      </div>
    );
  }

  return (
    <div {...getRootProps()} className={clsx("dropzone-gemini", isDragActive && "active")}>
      <input {...getInputProps()} />
      <div className="dropzone-icon-orb" aria-hidden="true">
        <FileUp size={24} strokeWidth={1.8} />
      </div>
      <div className="dropzone-title">
        {isDragActive ? "松开以添加文档" : title ?? "将 Word 或 Excel 文档拖入此处，或点击选择"}
      </div>
      <div className="dropzone-pills" aria-label={hint ?? "支持的文档格式"}>
        {OFFICE_EXTENSIONS.map((ext) => (
          <span key={ext} className="format-pill">
            {ext}
          </span>
        ))}
        <span className="format-pill subtle">→ PDF</span>
      </div>
      {rejectedNotice ? <div className="dropzone-error">{rejectedNotice}</div> : null}
    </div>
  );
}
