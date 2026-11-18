# AlexPDF

[简体中文](README.md)

A Tauri 2.x desktop app that converts .doc/.docx/.xls/.xlsx to PDF via Windows COM automation with WPS Office or Microsoft Office. Pure Rust backend, no Python/Node runtime required.

## Highlights

- Pure Rust + windows-rs COM automation
- Local HTTP API for UI or scripting
- Single and batch conversion (Word/Excel)
- Progress polling via `/api/status`
- Configurable PDF output folder

## Requirements

- Windows 10/11 (64-bit)
- A COM-enabled WPS Office installation or Microsoft Word / Excel

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

## License

Copyright (c) 2026 量子跃动. AlexPDF is distributed under the [MIT License](LICENSE). WPS Office and Microsoft Office are installed separately under their respective licenses.
