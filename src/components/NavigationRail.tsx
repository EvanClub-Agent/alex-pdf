import clsx from "clsx";
import {
  Activity,
  CheckCircle2,
  Clock,
  FileText,
  Layers,
  Link2,
  Loader2,
  Menu,
  Plus,
  Settings,
  XCircle
} from "lucide-react";
import type { TaskStatusResponse } from "../types";

export type NavKey = "workspace" | "history" | "diagnostics" | "settings";
export type ComposerMode = "local" | "url" | "batch";

type NavigationRailProps = {
  active: NavKey;
  mode: ComposerMode;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelectWorkspace: (mode: ComposerMode) => void;
  onNewConversion: () => void;
  onChangeNav: (key: NavKey) => void;
  recentTasks: TaskStatusResponse[];
  serverOk: boolean;
  port: number;
};

const modeItems: Array<{
  mode: ComposerMode;
  label: string;
  icon: typeof FileText;
}> = [
  { mode: "local", label: "本地文档转换", icon: FileText },
  { mode: "url", label: "单条链接转换", icon: Link2 },
  { mode: "batch", label: "批量链接转换", icon: Layers }
];

function summarizeTask(task: TaskStatusResponse): string {
  const firstFile = task.results?.[0]?.file || task.current_file;
  if (firstFile) {
    return task.total_files > 1 ? `${firstFile} 等 ${task.total_files} 个` : firstFile;
  }
  return `批量任务 (${task.completed_files}/${task.total_files})`;
}

export function NavigationRail({
  active,
  mode,
  collapsed,
  onToggleCollapse,
  onSelectWorkspace,
  onNewConversion,
  onChangeNav,
  recentTasks,
  serverOk,
  port
}: NavigationRailProps) {
  return (
    <aside className={clsx("gemini-sidebar", collapsed && "collapsed")} aria-label="主导航">
      <div className="sidebar-top">
        <button
          type="button"
          className="icon-btn sidebar-menu-btn"
          onClick={onToggleCollapse}
          aria-label={collapsed ? "展开侧边栏" : "收起侧边栏"}
          title={collapsed ? "展开侧边栏" : "收起侧边栏"}
        >
          <Menu size={20} />
        </button>

        <button
          type="button"
          className="new-task-btn"
          onClick={onNewConversion}
          title="新建 PDF 转换"
        >
          <Plus size={18} strokeWidth={2.25} />
          {!collapsed ? <span>新建转换</span> : null}
        </button>
      </div>

      <div className="sidebar-scroll">
        {!collapsed ? <div className="sidebar-section-title">转换模式</div> : null}
        <nav className="sidebar-nav-group" aria-label="转换模式">
          {modeItems.map((item) => {
            const Icon = item.icon;
            const isActive = active === "workspace" && mode === item.mode;
            return (
              <button
                key={item.mode}
                type="button"
                className={clsx("sidebar-item", isActive && "active")}
                onClick={() => onSelectWorkspace(item.mode)}
                title={collapsed ? item.label : undefined}
              >
                <span className="sidebar-item-icon">
                  <Icon size={18} />
                </span>
                {!collapsed ? <span className="sidebar-item-label">{item.label}</span> : null}
              </button>
            );
          })}
        </nav>

        {!collapsed ? (
          <div className="sidebar-recent">
            <div className="sidebar-section-header">
              <span className="sidebar-section-title">最近任务</span>
              <button
                type="button"
                className="text-action-btn"
                onClick={() => onChangeNav("history")}
              >
                全部
              </button>
            </div>
            {recentTasks.length === 0 ? (
              <div className="sidebar-empty-hint">暂无近期转换记录</div>
            ) : (
              <div className="sidebar-recent-list">
                {recentTasks.slice(0, 5).map((task) => {
                  const isOk = task.status === "completed";
                  const isRunning = task.status === "processing" || task.status === "pending";
                  return (
                    <button
                      key={task.task_id}
                      type="button"
                      className="sidebar-recent-item"
                      onClick={() => onChangeNav("history")}
                      title={summarizeTask(task)}
                    >
                      <span className="recent-status-icon" aria-hidden="true">
                        {isRunning ? (
                          <Loader2 size={14} className="spin" />
                        ) : isOk ? (
                          <CheckCircle2 size={14} className="ok" />
                        ) : (
                          <XCircle size={14} className="err" />
                        )}
                      </span>
                      <span className="sidebar-item-label">{summarizeTask(task)}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        ) : null}
      </div>

      <div className="sidebar-bottom">
        <button
          type="button"
          className={clsx("sidebar-item", active === "history" && "active")}
          onClick={() => onChangeNav("history")}
          title={collapsed ? "历史记录" : undefined}
        >
          <span className="sidebar-item-icon">
            <Clock size={18} />
          </span>
          {!collapsed ? <span className="sidebar-item-label">历史记录</span> : null}
        </button>

        <button
          type="button"
          className={clsx("sidebar-item", active === "diagnostics" && "active")}
          onClick={() => onChangeNav("diagnostics")}
          title={collapsed ? "系统诊断" : undefined}
        >
          <span className="sidebar-item-icon">
            <Activity size={18} />
          </span>
          {!collapsed ? <span className="sidebar-item-label">系统诊断</span> : null}
        </button>

        <button
          type="button"
          className={clsx("sidebar-item", active === "settings" && "active")}
          onClick={() => onChangeNav("settings")}
          title={collapsed ? "设置" : undefined}
        >
          <span className="sidebar-item-icon">
            <Settings size={18} />
          </span>
          {!collapsed ? <span className="sidebar-item-label">设置</span> : null}
        </button>

        {!collapsed ? (
          <div className="sidebar-location-bar">
            <span className={clsx("status-dot", serverOk ? "ok" : "error")} aria-hidden="true" />
            <span>本地引擎 127.0.0.1:{port}</span>
          </div>
        ) : null}
      </div>
    </aside>
  );
}
