type LogPanelProps = {
  logs: string[];
};

export function LogPanel({ logs }: LogPanelProps) {
  if (!logs.length) {
    return <div className="log-panel empty">等待处理任务...</div>;
  }

  return (
    <div className="log-panel">
      {logs.map((line, index) => {
        let type = "info";
        if (line.includes("错误") || line.includes("失败")) type = "error";
        if (line.includes("完成") || line.includes("成功")) type = "success";

        return (
          <div className={`log-line ${type}`} key={`${line}-${index}`}>
            {line}
          </div>
        );
      })}
    </div>
  );
}
