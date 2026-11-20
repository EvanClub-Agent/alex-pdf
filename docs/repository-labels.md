# AlexPDF 开源仓库标签规范

AlexPDF 使用 Windows 本机的 WPS Office 或 Microsoft Office 将 Word、Excel 转为 PDF。维护者在 GitHub 的以下位置标注项目用途、支持范围、协作事项和发布版本。

| 读者想知道什么 | 维护位置 | AlexPDF 示例 |
| --- | --- | --- |
| 搜索什么词能找到项目 | 仓库首页 → About → Topics | `word-to-pdf` |
| 许可证、平台和构建状态是什么 | 两份 README 的标题下方 | MIT、Windows 10/11 x64、Windows build |
| 这个 Issue 或 PR 需要什么处理 | Issues / Pull requests → Labels | `bug` |
| 安装包对应哪一次源码提交 | Git tag 与 GitHub Release | `v1.2.3`（格式示例） |

## 仓库首页与 Topics

**建议的 About 描述**

Windows 本地 Word / Excel 转 PDF 工具，支持 WPS Office、Microsoft Office、批量转换与 HTTP API。

**建议的 Topics**（在 About 中逐项添加）

```text
pdf-converter word-to-pdf excel-to-pdf batch-conversion windows wps-office microsoft-office tauri rust
```

| 分类 | 为什么保留这些词 |
| --- | --- |
| 用途 | `pdf-converter`、`word-to-pdf`、`excel-to-pdf` 和 `batch-conversion` 对应用户能完成的转换任务。 |
| 运行环境 | `windows` 描述转换平台。`wps-office`、`microsoft-office` 对应可选的本机 COM 引擎。 |
| 实现 | `tauri` 和 `rust` 帮助贡献者找到桌面应用及其转换实现。 |

GitHub 每仓库最多允许 20 个 Topics，每个最长 50 个字符，仅使用小写字母、数字和连字符。维护者在 About 中保存后，Topics 才会显示在仓库首页。增删平台或格式支持时，依据桌面实现和转换验证重新核对这些词。截图的窗口外观不能作为平台支持的依据。[GitHub Topics 说明](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/classifying-your-repository-with-topics)

## README 徽章与图片

| 徽章 | 依据 | 点击后应到达 |
| --- | --- | --- |
| License · MIT | 仓库根目录的授权文本 | [LICENSE](../LICENSE) |
| Windows build | GitHub Actions 返回的 `main` 分支状态 | [Windows 构建记录](https://github.com/EvanClub-Agent/alex-pdf/actions/workflows/windows.yml) |
| Windows · 10 / 11 x64 | 桌面端的系统要求和 COM 依赖 | 当前语言 README 的系统要求 |

两个语言版本应使用同一组徽章和同一张[工作台截图](assets/alexpdf-workbench.png)。徽章提供可读的替代文本，点击截图可查看原图。加入下载或版本徽章前，先确认相应 Release 及安装包对目标读者可访问。私有仓库的 CI 状态可能需要有权限的账户才能查看。

社交分享图采用 1280 × 640 像素。修改排版时编辑 [SVG 源文件](assets/social-preview.svg)，再导出小于 1 MB 的 [PNG 预览图](assets/social-preview.png)。在公开仓库的 Settings → Social preview 上传 PNG 即可。私有仓库仅在此前已上传过预览图时可更换图片，对外分享需要仓库公开。[GitHub 分享预览说明](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/customizing-your-repositorys-social-media-preview)

## Issue 与 PR 标签

| 标签 | 何时使用 |
| --- | --- |
| `bug` | 已有功能出现异常。例如合规的工作簿在可用引擎上导出失败。问题反馈模板会自动添加。 |
| `enhancement` | 请求新能力或改进现有体验。功能建议模板会自动添加。 |
| `documentation` | 修改 README、接口示例、截图或贡献说明。 |
| `question` | 需要解释配置、平台要求或操作方式。 |
| `good first issue` | 维护者已经写明修改范围和验收方式，适合第一次参与。 |
| `help wanted` | 需要协助，例如请具有指定 WPS 版本的贡献者验证结果。 |

维护者在分流时按需组合标签。例如给一项已有明确修订方案的文档问题添加 `documentation` 和 `good first issue`。调整 `bug` 或 `enhancement` 名称时，同步修改对应 [Issue 模板](../.github/ISSUE_TEMPLATE)。安全问题请走[私密报告渠道](../SECURITY.md)。[GitHub Labels 说明](https://docs.github.com/en/issues/using-labels-and-milestones-to-track-work/managing-labels)

## 发布用的 Git 标签

正式版本标签采用 `v主版本.次版本.修订号`。例如安装包版本为 `1.2.3`，对应 Git 标签为 `v1.2.3`。以下是维护者发版时的检查顺序：

1. 使用 `npm run bump -- <版本号>` 同步 [package.json](../package.json)、[Cargo.toml](../src-tauri/Cargo.toml)、[tauri.conf.json](../src-tauri/tauri.conf.json) 及两个锁文件。更新[变更记录](../CHANGELOG.md)。
2. 确认目标提交通过 Windows 测试和 NSIS 打包，用该提交构建的安装包验证转换。
3. 在该提交上创建带注释的 Git 标签。GitHub Release 使用同一标签并附上对应的 Windows 安装包。仓库现有 [Windows 工作流](../.github/workflows/windows.yml) 负责测试和打包，Release 由维护者发布。
4. 使用自托管更新服务时，再核对清单中的版本号、下载地址和 SHA-256。[更新服务文档](../update-server/README.md)

## 开源仓库交付清单

- [ ] [中文首页](../README.md)和[英文首页](../README.en.md)均有用途、系统要求、可执行的上手步骤、真实截图和可访问的徽章链接。
- [ ] About 描述与 Topics 反映已实现的功能，社交预览在仓库对外分享时可见。
- [ ] [许可证](../LICENSE)、[贡献指南](../CONTRIBUTING.md)、[行为准则](../CODE_OF_CONDUCT.md)和[安全策略](../SECURITY.md)可从首页找到。
- [ ] 问题反馈、功能建议与 [PR 模板](../.github/PULL_REQUEST_TEMPLATE.md)对应现有标签和检查命令。
- [ ] 发布时，Git 标签、版本配置、变更记录及可下载的安装包对应同一提交。
