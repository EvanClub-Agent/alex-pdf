# AlexPDF 安全策略

## 私下报告

如果问题涉及未经授权的文件访问、API 令牌、文档下载、安装包更新或发布后台，请私下联系仓库维护者。邮件标题请写 `[AlexPDF Security]`，便于识别。Please report security issues privately to the maintainer. [ai.agent.club@gmail.com](mailto:ai.agent.club@gmail.com)

请说明受影响的 AlexPDF 版本或提交、Windows 和 Office/WPS 版本、相关组件、触发条件和观察到的影响。用自行构造且已脱敏的样本说明问题。请将未修复漏洞的细节保留在私下交流中。

## 诊断材料

公开反馈前，请移除真实的 Word、Excel 和 PDF 文件、API 或管理令牌、签名下载地址，以及日志中的用户名和文件路径。本机转换 API 默认监听 `127.0.0.1:24568`。可选更新服务有单独的管理凭据和下载日志，详见[更新服务说明](update-server/README.md)。
