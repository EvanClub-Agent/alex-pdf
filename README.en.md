<div align="center">

<img src="src-tauri/icons/128x128.png" width="64" alt="AlexPDF icon" />

# AlexPDF

**Convert Word documents and Excel workbooks to PDF on Windows.**

AlexPDF exports through your installed copy of WPS Office or Microsoft Office. Drop in local files, paste direct download links, or submit a job from a script through the local HTTP API.

[![MIT license](https://img.shields.io/badge/license-MIT-215780)](LICENSE)
[![Windows build status](https://github.com/EvanClub-Agent/alex-pdf/actions/workflows/windows.yml/badge.svg?branch=main)](https://github.com/EvanClub-Agent/alex-pdf/actions/workflows/windows.yml)
[![Windows 10 and 11 x64](https://img.shields.io/badge/Windows-10%20%2F%2011%20x64-2559A4)](#requirements)

[简体中文](README.md) · **English**

[Quick start](#quick-start) · [HTTP API](#api) · [Contributing](CONTRIBUTING.md) · [Repository labels](docs/repository-labels.md)

[![AlexPDF workbench in Chinese, showing local files, single links, batch links, and the drop zone for Word and Excel documents.](docs/assets/alexpdf-workbench.png)](docs/assets/alexpdf-workbench.png)

</div>

## Highlights

- Tauri 2 desktop app with a Rust backend and windows-rs COM automation. The packaged desktop app runs without Node.js or Python.
- Local HTTP API for UI or scripting
- Single and batch conversion (Word/Excel)
- Progress polling via `/api/status/{task_id}`
- Configurable PDF output folder

## Requirements

- Windows 10/11 (64-bit)
- A COM-enabled WPS Office installation or Microsoft Word / Excel

## Quick start

1. Build and launch the desktop app on Windows using the commands below. Users with an installer only need the app and the corresponding Office engine.
2. Open **系统诊断** (Diagnostics) to check engine availability. Word files need a word processor and Excel files need a spreadsheet engine.
3. In **本地文档转换** (Local files), drop in a `.docx` or `.xlsx` file, pick an output folder, and click **开始转换** (Convert). Submit Word and Excel as separate batches.
4. For remote documents, choose single-link mode or paste one direct download URL per line in batch-link mode.

## Development

Install Node.js 22.12+, stable Rust with the MSVC toolchain, Visual Studio C++ Build Tools and WebView2. See the [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows).

```bash
npm ci

npm run tauri dev
npm run tauri build
```

The installer is output to: `src-tauri/target/release/bundle/nsis/*.exe`

## API

Service listens on `127.0.0.1:24568`. Visit `/api/docs` for interactive documentation or `/api/openapi.json` for the schema. If API token authentication is enabled, include the `Authorization: Bearer <token>` header.

### 1) Health

```
GET /health
```

Response:

```json
{
  "status": "ok",
  "version": "1.0.0",
  "wps_available": true,
  "wps_prog_id": "KWPS.Application"
}
```

### 2) Check WPS COM

```
GET /api/check-wps
```

### 3) Single Convert

```
POST /api/convert
```

```bash
curl -X POST http://127.0.0.1:24568/api/convert \
  -F "file=@C:\\test\\document.docx" \
  -F "output_name=output.pdf"
```

### 4) Batch Convert

```
POST /api/convert-batch
```

```bash
curl -X POST http://127.0.0.1:24568/api/convert-batch \
  -F "files=@C:\\test\\doc1.docx" \
  -F "files=@C:\\test\\doc2.doc" \
  -F "output_folder=C:\\output"
```

### 5) Async Progress

When calling convert endpoints with `?async=true`, use:

```
GET /api/status/{task_id}
```

## Test Scripts

- PowerShell: `scripts/test_api.ps1`
- Bash: `scripts/test_api.sh`

## Notes & Limits

- Localhost only (`127.0.0.1`)
- Only `.doc/.docx/.xls/.xlsx`
- Batch conversion does not allow mixed Word/Excel types
- Default file size limit: 50MB
- WPS COM availability varies by edition

## Updates

The client checks for updates when `PDF_CONVERTER_UPDATE_URL` points to a release manifest. Use HTTPS for remote deployments or HTTP on loopback for local testing. The client verifies the installer against the SHA-256 in that manifest. See the [update server guide](update-server/README.md).

## Community

For setup and checks before opening a pull request, see the [contribution guide](CONTRIBUTING.md). For repository maintenance, see the [label guide](docs/repository-labels.md) and [changelog](CHANGELOG.md). Community participation follows the [code of conduct](CODE_OF_CONDUCT.md). Report security issues privately using the [security policy](SECURITY.md).

## License

Copyright (c) 2026 量子跃动. AlexPDF is distributed under the [MIT License](LICENSE). WPS Office and Microsoft Office are installed separately under their respective licenses.
