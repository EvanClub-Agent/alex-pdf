import { useCallback, useMemo, useState } from "react";

import type { FileItem } from "../types";

function toKey(file: File): string {
  return `${file.name}-${file.size}-${file.lastModified}`;
}

export function useFileUpload() {
  const [files, setFiles] = useState<FileItem[]>([]);

  const addFiles = useCallback((incoming: File[]) => {
    setFiles((prev) => {
      const existing = new Set(prev.map((item) => toKey(item.file)));
      const next = [...prev];

      for (const file of incoming) {
        const key = toKey(file);
        if (existing.has(key)) {
          continue;
        }
        existing.add(key);
        next.push({
          id: crypto.randomUUID(),
          file,
          name: file.name,
          size: file.size,
          status: "ready",
          selected: true
        });
      }

      return next;
    });
  }, []);

  const clearFiles = useCallback(() => {
    setFiles([]);
  }, []);

  const removeFile = useCallback((id: string) => {
    setFiles((prev) => prev.filter((item) => item.id !== id));
  }, []);

  const toggleSelect = useCallback((id: string) => {
    setFiles((prev) =>
      prev.map((item) =>
        item.id === id ? { ...item, selected: !item.selected } : item
      )
    );
  }, []);

  const selectAll = useCallback((value: boolean) => {
    setFiles((prev) => prev.map((item) => ({ ...item, selected: value })));
  }, []);

  const updateFiles = useCallback((updater: (prev: FileItem[]) => FileItem[]) => {
    setFiles((prev) => updater(prev));
  }, []);

  const selectedCount = useMemo(
    () => files.filter((item) => item.selected).length,
    [files]
  );

  return {
    files,
    setFiles,
    addFiles,
    clearFiles,
    removeFile,
    toggleSelect,
    selectAll,
    updateFiles,
    selectedCount
  };
}
