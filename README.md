<div align="center">

<img src="src-tauri/icons/128x128.png" width="64" alt="AlexPDF 图标" />

# AlexPDF

**在 Windows 上，把 Word 和 Excel 批量转为 PDF。**

使用本机安装的 WPS Office 或 Microsoft Office 排版导出。可以拖入文件、粘贴文档直链，也可以从脚本调用本机 HTTP API。

[![MIT 许可证](https://img.shields.io/badge/license-MIT-215780)](LICENSE)
[![Windows 构建状态](https://github.com/EvanClub-Agent/alex-pdf/actions/workflows/windows.yml/badge.svg?branch=main)](https://github.com/EvanClub-Agent/alex-pdf/actions/workflows/windows.yml)
[![支持 Windows 10 和 11 x64](https://img.shields.io/badge/Windows-10%20%2F%2011%20x64-2559A4)](#系统要求)

**简体中文** · [English](README.en.md)

[快速上手](#快速上手) · [API 文档](#api-文档) · [参与贡献](CONTRIBUTING.md) · [仓库标签规范](docs/repository-labels.md)

[![AlexPDF 主界面：左侧切换本地文档、单条链接与批量链接，中央可拖入 Word 或 Excel 并选择 PDF 输出目录。](docs/assets/alexpdf-workbench.png)](docs/assets/alexpdf-workbench.png)

</div>

## 亮点

- Tauri 2 桌面应用，Rust 后端通过 windows-rs 调用 COM，安装后的桌面端无需 Node.js 或 Python
- 本地 HTTP API，前端/脚本均可调用
- 支持单文件/批量转换（Word/Excel）
- 通过 `/api/status/{task_id}` 按任务查询转换进度
- 支持在安装时或转换时指定 PDF 输出目录

## 系统要求

- Windows 10/11 (64 位)
- 已安装可提供 COM 接口的 WPS Office，或 Microsoft Word / Excel

## 快速上手

1. 在 Windows 上按下方「开发与运行」构建并启动 AlexPDF。使用安装包的用户只需安装应用及对应的 Office 引擎。
2. 打开「系统诊断」查看转换引擎。转换 Word 文档需要文字处理组件，转换 Excel 表格需要电子表格组件。
3. 回到「本地文档转换」，拖入一份 `.docx` 或 `.xlsx` 文件，选择输出目录后点击「开始转换」。批量处理时将 Word 和 Excel 分开提交。
4. 文档来自下载地址时，选择「单条链接转换」；处理多条直链时，在「批量链接转换」中每行粘贴一条。

## 开发与运行

准备 Node.js 22.12+、Rust stable（MSVC 工具链）、Visual Studio C++ 生成工具和 WebView2。详见 [Tauri Windows 开发环境](https://v2.tauri.app/start/prerequisites/#windows)。

```bash
# 安装前端依赖
npm ci

# 开发模式
npm run tauri dev

# 生产构建
npm run tauri build
```

构建完成的安装包位于：`src-tauri/target/release/bundle/nsis/*.exe`

## 版本号管理 / 发版流程

桌面工具的版本号记录在下列三个文件中。发版脚本也会同步更新 npm 和 Cargo 锁文件。

| 文件 | 字段 | 用途 |
|---|---|---|
| `package.json` | `version` | npm 包元数据 |
| `src-tauri/Cargo.toml` | `[package].version` | Rust 端 `env!("CARGO_PKG_VERSION")`，决定客户端检查更新比对结果 |
| `src-tauri/tauri.conf.json` | `version` | NSIS 安装包元数据（控制面板里显示的版本） |

三个版本号应保持一致，以便客户端检查更新时使用的版本与安装包显示的版本相符。

### 一键改版本号

仓库提供了 `scripts/bump-version.mjs` 一次性同步所有版本号，已注册为 npm script：

```bash
# 显式指定版本号
npm run bump 1.0.1

# 按语义化版本递增
npm run bump patch     # 1.0.0 -> 1.0.1（修复发布）
npm run bump minor     # 1.0.0 -> 1.1.0（新功能）
npm run bump major     # 1.0.0 -> 2.0.0（破坏性变更）
```

输出：

```
✓ package.json          1.0.0 → 1.0.1
✓ src-tauri/Cargo.toml  1.0.0 → 1.0.1
✓ src-tauri/tauri.conf.json  1.0.0 → 1.0.1
```

### 一条龙发版

```bash
# 等价于 npm run bump patch && npm run tauri build
npm run release
```

### 完整发版流程

1. **改版本号**：`npm run bump <version|patch|minor|major>`
2. **打包**（Windows 上）：`npm run tauri build`
3. **分发**：发布 GitHub Release 时，按[仓库标签规范](docs/repository-labels.md)为已验证的提交创建版本标签并附上安装包。使用自托管更新服务时，在服务的 `/admin` 上传安装包并检查 `/update.json` 中的版本和 SHA-256。
4. **检查更新**：启动客户端前设置 `PDF_CONVERTER_UPDATE_URL`。清单和安装包的远程地址使用 HTTPS，本机测试可使用 `http://127.0.0.1:8800/update.json`。留空时桌面转换仍可使用，客户端不访问更新服务器。

部署参数和下载日志说明见 [更新服务文档](update-server/README.md)。

## API 文档

服务监听 `127.0.0.1:24568`，浏览器打开 `/api/docs` 可查看完整接口，也可以从 `/api/openapi.json` 获取 OpenAPI 定义。启用设置中的 API 令牌后，请求需携带 `Authorization: Bearer <token>`。

### 1) 健康检查

```
GET /health
```

响应示例：

```json
{
  "status": "ok",
  "version": "1.0.0",
  "wps_available": true,
  "wps_prog_id": "KWPS.Application"
}
```

### 2) 检查 WPS COM

```
GET /api/check-wps
```

响应示例：

```json
{
  "available": true,
  "prog_id": "KWPS.Application",
  "version": null,
  "error": null
}
```

如需查询引擎版本，可调用 `GET /api/engine-version?engine=wps`。

### 3) 单文件转换

```
POST /api/convert
Content-Type: multipart/form-data
```

参数：
- `file`：Word 或 Excel 文档（必需）
- `output_name`：输出文件名（可选）

同步示例：

```bash
curl -X POST http://127.0.0.1:24568/api/convert \
  -F "file=@C:\\test\\document.docx" \
  -F "output_name=output.pdf"
```

响应示例：

```json
{
  "success": true,
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "input_file": "document.docx",
  "output_file": "C:\\Temp\\wps-pdf-output\\output.pdf",
  "file_size": 1024576,
  "duration_ms": 1250,
  "message": "转换成功",
  "error": null
}
```

### 4) 批量转换

```
POST /api/convert-batch
Content-Type: multipart/form-data
```

参数：
- `files` 或 `files[]`：多个同类型的 Word 或 Excel 文档
- `output_folder`：输出文件夹路径（可选，需为绝对路径）

同步示例：

```bash
curl -X POST http://127.0.0.1:24568/api/convert-batch \
  -F "files=@C:\\test\\doc1.docx" \
  -F "files=@C:\\test\\doc2.doc" \
  -F "output_folder=C:\\output"
```

响应示例：

```json
{
  "success": true,
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "total": 2,
  "succeeded": 2,
  "failed": 0,
  "duration_ms": null,
  "results": [
    {
      "file": "doc1.docx",
      "status": "success",
      "output": "C:\\output\\doc1.pdf",
      "size": 512000,
      "duration_ms": 980,
      "error": null
    }
  ],
  "message": "转换完成"
}
```

### 5) 进度查询（异步）

当请求带 `?async=true` 参数时，接口会返回 `task_id` 并立即响应，前端可轮询：

```
GET /api/status/{task_id}
```

响应示例：

```json
{
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "processing",
  "progress": 60,
  "current_file": "doc3.docx",
  "total_files": 5,
  "completed_files": 3,
  "results": null,
  "error": null,
  "started_at": "2024-01-01T12:00:00+08:00"
}
```

## 测试脚本

- PowerShell 脚本：`scripts/test_api.ps1`
- Bash 脚本：`scripts/test_api.sh`

## 安全与限制

- 服务仅监听 `127.0.0.1`
- 文件类型仅允许 `.doc/.docx/.xls/.xlsx`
- 批量转换不支持混合 Word/Excel 类型
- 默认单个文件大小限制 50MB
- WPS 并发能力有限，建议批量转换保持低并发

## 项目结构

```
src-tauri/          Rust 后端与 Tauri
src/                React 前端
scripts/            API 测试脚本
```

## 备注

- 仅支持 Windows 平台（依赖 Office 或 WPS 的 COM 接口）
- 个人免费版 WPS 可能不包含 COM 接口能力

## 社区

贡献代码或文档请阅读[贡献指南](CONTRIBUTING.md)和[行为准则](CODE_OF_CONDUCT.md)。版本变更见[变更记录](CHANGELOG.md)，安全问题请按[安全策略](SECURITY.md)私下反馈。

## 许可证

Copyright (c) 2026 量子跃动。AlexPDF 源码采用 [MIT 许可证](LICENSE)。WPS Office 和 Microsoft Office 由用户单独安装，遵循各自的许可条款。
