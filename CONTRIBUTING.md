# 参与 AlexPDF 开发

AlexPDF 的 React 界面位于 `src/`，本机 HTTP 服务和 Office 自动化位于 `src-tauri/src/`，可选的安装包分发服务位于 `update-server/`。开始前请阅读[行为准则](CODE_OF_CONDUCT.md)。

## 运行项目

准备 Node.js 22.12 或更高版本。运行桌面端需要 Windows 10/11 x64、Rust stable MSVC 工具链、Visual Studio C++ 生成工具和 WebView2。实际转换还需要具有 COM 接口的 WPS，或文档类型对应的 Microsoft Word / Excel。[Tauri 的 Windows 环境说明](https://v2.tauri.app/start/prerequisites/#windows)

```sh
git clone https://github.com/EvanClub-Agent/alex-pdf.git
cd alex-pdf
npm ci
npm run tauri dev
```

只修改前端时，可用 `npm run dev` 预览页面，运行 `npm run build` 做 TypeScript 检查并生成前端产物。独立启动 Vite 时，本机 API 和 Tauri 桌面能力需要另行启动桌面应用才能验证。更新服务有独立的[运行说明](update-server/README.md)。

## 如何验证改动

| 改动 | 检查方式 |
| --- | --- |
| README 或截图 | 在 Markdown 预览中打开中英文首页，点击截图、徽章和内部链接。 |
| React 界面 | 运行 `npm run build`。交互改动请在桌面应用中验证，并附上相关截图。 |
| Rust 或本机 API | 运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml`。改动转换逻辑时，在 Windows 使用无敏感信息的小样本验证，并注明引擎、输入格式及结果。 |
| 安装或打包 | 在 Windows 运行 `npm run tauri build -- --bundles nsis -- --locked`，检查 `src-tauri/target/release/bundle/nsis/` 中的安装包。 |

Windows CI 会进行前端构建、Rust 测试和安装包构建。若问题只在某个 Office/WPS 版本中出现，请在反馈中给出该版本。可直接查看 [Windows CI 配置](.github/workflows/windows.yml)。

## 提交 Issue 或 PR

- 报告转换问题时，请说明使用本地文件还是链接、文件扩展名、引擎版本、复现步骤和预期 PDF 的表现。请先移除原始文档、带签名的下载链接和日志中的个人信息。
- PR 请说明解决的问题、涉及的模块及实际执行的检查。首次参与可从修正文档示例或标有 `good first issue` 的问题开始。
- 维护者更新仓库首页、分类或版本时，请参见[开源仓库标签规范](docs/repository-labels.md)和[变更记录](CHANGELOG.md)。安全问题请按[安全策略](SECURITY.md)私下报告。
