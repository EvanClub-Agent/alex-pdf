import clsx from "clsx";
import { ArrowUpCircle, EyeOff, Globe } from "lucide-react";

type StatusBarProps = {
  serverOk: boolean;
  version?: string;
  onVersionClick?: () => void;
  versionChecking?: boolean;
  updateAvailable?: boolean;
  latestVersion?: string | null;
  onOpenWebConsole?: () => void;
  onHideToTray?: () => void;
};

export function StatusBar({
  serverOk,
  version,
  onVersionClick,
  versionChecking,
  updateAvailable,
  latestVersion,
  onOpenWebConsole,
  onHideToTray
}: StatusBarProps) {
  const versionLabel = version
    ? versionChecking
      ? `检查更新中…`
      : updateAvailable && latestVersion
      ? `发现 v${latestVersion}`
      : `v${version}`
    : "";

  return (
    <div className="topbar-controls">
      <div
        className={clsx("topbar-pill", serverOk ? "online" : "offline")}
        title={serverOk ? "本地转换服务运行正常" : "本地转换服务未连接"}
      >
        <span className={clsx("status-dot", serverOk ? "ok" : "error")} aria-hidden="true" />
        <span>{serverOk ? "服务就绪" : "服务离线"}</span>
      </div>

      {version && onVersionClick ? (
        <button
          className={clsx("topbar-pill clickable", updateAvailable && "update-ready")}
          onClick={onVersionClick}
          type="button"
          title={updateAvailable ? "点击查看并安装新版本" : "点击检查更新"}
        >
          {updateAvailable ? <ArrowUpCircle size={14} aria-hidden="true" /> : null}
          <span>{versionLabel}</span>
        </button>
      ) : null}

      {onOpenWebConsole ? (
        <button
          type="button"
          className="icon-btn"
          onClick={onOpenWebConsole}
          title="打开本地 Web 控制台"
          aria-label="打开本地 Web 控制台"
        >
          <Globe size={18} />
        </button>
      ) : null}

      {onHideToTray ? (
        <button
          type="button"
          className="icon-btn"
          onClick={onHideToTray}
          title="最小化到系统托盘"
          aria-label="最小化到系统托盘"
        >
          <EyeOff size={18} />
        </button>
      ) : null}
    </div>
  );
}
