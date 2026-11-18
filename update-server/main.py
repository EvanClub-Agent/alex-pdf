"""FastAPI 服务：发布安装包 + 支持 HTTP Range 断点续传 + 下载日志落库。"""
from __future__ import annotations

import hashlib
import os
import re
import secrets
from contextlib import asynccontextmanager
from pathlib import Path
from urllib.parse import quote
from typing import Optional
from uuid import uuid4

# database 内部已经 load_dotenv，这里再 import 触发它即可保证 STORAGE_DIR / HOST / PORT 都已生效
from fastapi import Depends, FastAPI, File, Form, Header, HTTPException, Query, Request, UploadFile
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse, PlainTextResponse, Response, StreamingResponse
from fastapi.staticfiles import StaticFiles
from sqlalchemy import select, text, update
from sqlalchemy.ext.asyncio import AsyncSession

from database import Base, engine, get_db
from models import DownloadSession, Release


STORAGE_DIR = Path(os.getenv("STORAGE_DIR", "./storage")).resolve()
STORAGE_DIR.mkdir(parents=True, exist_ok=True)

CHUNK_SIZE = 64 * 1024
RANGE_PATTERN = re.compile(r"^bytes=(\d*)-(\d*)$", re.IGNORECASE)

ADMIN_TOKEN = os.getenv("ADMIN_TOKEN", "").strip()


async def require_admin(authorization: Optional[str] = Header(default=None)) -> None:
    """所有 /admin/* 接口的鉴权依赖。"""
    if not ADMIN_TOKEN:
        raise HTTPException(status_code=503, detail="管理接口已禁用，请配置 ADMIN_TOKEN")
    if not authorization or not authorization.lower().startswith("bearer "):
        raise HTTPException(status_code=401, detail="缺少 Authorization Bearer token")
    provided = authorization.split(" ", 1)[1].strip()
    if not secrets.compare_digest(provided.encode(), ADMIN_TOKEN.encode()):
        raise HTTPException(status_code=403, detail="无效的 admin token")


@asynccontextmanager
async def lifespan(app: FastAPI):
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    if not ADMIN_TOKEN:
        print("[update-server] ADMIN_TOKEN 未设置，管理接口已禁用")
    yield
    await engine.dispose()


app = FastAPI(title="AlexPDF · 更新服务", lifespan=lifespan)

STATIC_DIR = Path(__file__).resolve().parent / "static"
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")


@app.get("/", response_class=HTMLResponse)
async def landing_page():
    """官网首页 —— 产品介绍、功能、下载、API 文档、FAQ。"""
    html_path = STATIC_DIR / "index.html"
    if not html_path.is_file():
        raise HTTPException(status_code=500, detail="缺少 index.html")
    return HTMLResponse(html_path.read_text(encoding="utf-8"))


@app.get("/admin", response_class=HTMLResponse)
async def admin_index():
    """管理后台 —— 版本维护 + 安装包上传 + 下载日志。"""
    html_path = STATIC_DIR / "admin.html"
    if not html_path.is_file():
        raise HTTPException(status_code=500, detail="缺少 admin.html")
    return HTMLResponse(html_path.read_text(encoding="utf-8"))


# --------------------------------------------------------------------------- #
# SEO / GEO：robots.txt、sitemap.xml、llms.txt 动态生成
# --------------------------------------------------------------------------- #


@app.get("/robots.txt", response_class=PlainTextResponse, include_in_schema=False)
async def robots_txt(request: Request):
    base = str(request.base_url).rstrip("/")
    return (
        "User-agent: *\n"
        "Allow: /\n"
        "Allow: /update.json\n"
        "Disallow: /admin\n"
        "Disallow: /admin/\n"
        "Disallow: /downloads/\n"
        "\n"
        "# 允许常见 AI 爬虫做内容收录（GEO / 生成式搜索友好）\n"
        "User-agent: GPTBot\nAllow: /\n"
        "User-agent: ClaudeBot\nAllow: /\n"
        "User-agent: anthropic-ai\nAllow: /\n"
        "User-agent: PerplexityBot\nAllow: /\n"
        "User-agent: Google-Extended\nAllow: /\n"
        "User-agent: Bytespider\nAllow: /\n"
        f"\nSitemap: {base}/sitemap.xml\n"
    )


@app.get("/sitemap.xml", include_in_schema=False)
async def sitemap_xml(request: Request):
    base = str(request.base_url).rstrip("/")
    today = "2026-05-07"
    sections = ["", "#features", "#how", "#cases", "#api", "#requirements", "#download", "#faq"]
    urls = "\n".join(
        f"  <url>\n"
        f"    <loc>{base}/{anchor}</loc>\n"
        f"    <lastmod>{today}</lastmod>\n"
        f"    <changefreq>weekly</changefreq>\n"
        f"    <priority>{'1.0' if anchor == '' else '0.7'}</priority>\n"
        f"  </url>"
        for anchor in sections
    )
    body = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        f"{urls}\n"
        "</urlset>\n"
    )
    return Response(content=body, media_type="application/xml")


@app.get("/llms.txt", response_class=PlainTextResponse, include_in_schema=False)
async def llms_txt(request: Request):
    """GEO 优化：给 LLM / 生成式搜索引擎一份结构化的「产品说明书」。"""
    base = str(request.base_url).rstrip("/")
    return (
        "# AlexPDF\n"
        "\n"
        "> 基于 Tauri 2 + Rust + WPS COM 的 Windows 桌面工具，"
        "支持 .doc / .docx / .xls / .xlsx 批量高保真转 PDF，"
        "100% 本地运行、文档不出本机、内置本地 HTTP API。\n"
        "\n"
        "## 关键事实\n"
        "\n"
        "- 平台：仅支持 Windows 10 / 11（64 位）\n"
        "- 依赖：本机已安装 WPS Office（推荐专业版/企业版以确保 COM 接口可用）\n"
        "- 技术栈：Tauri 2、Rust、Axum、React 18、TypeScript、Vite、windows-rs\n"
        "- 转换引擎：WPS Office COM 接口（KWPS.Application），自动回退到 MS Word.Application\n"
        "- 价格：完全免费、无水印、无文件数与页数限制\n"
        "- 隐私：所有转换在本机完成，HTTP 服务仅监听 127.0.0.1\n"
        "- 单文件大小上限：50 MB；批量请求上限：120 MB\n"
        "- 并发限制：1（WPS COM 非线程安全，由 Semaphore 串行化）\n"
        "\n"
        "## 主要功能\n"
        "\n"
        "- 批量将 Word / Excel 文档转换为 PDF\n"
        "- 异步任务模型：返回 task_id 后通过 /api/status/{task_id} 轮询进度\n"
        "- 自定义输出目录（绝对路径），未指定时落到系统临时目录\n"
        "- 自动检查更新 + HTTP Range 断点续传安装包下载\n"
        "- 内置 HTTP API（127.0.0.1:24568），可被 PowerShell / Python / Node 调用\n"
        "\n"
        "## 链接\n"
        "\n"
        f"- 官网首页：{base}/\n"
        f"- 下载元数据（JSON）：{base}/update.json\n"
        f"- 健康检查：{base}/health\n"
        f"- 站点地图：{base}/sitemap.xml\n"
        "\n"
        "## 适用场景\n"
        "\n"
        "- 企业批量归档：合同、报表、人事档案统一打包成 PDF\n"
        "- 财务报表分发：把 Excel 报表转为 PDF 后邮件发送，避免接收方误改\n"
        "- 自动化办公：通过本地 HTTP API 接入 RPA、CI、定时脚本\n"
        "- 合同审批：OA 流转前先把 docx 锁成 PDF，避免字体丢失/版式错位\n"
        "- 教学培训：批量导出课件、习题册供学员跨设备阅读\n"
        "\n"
        "## 不适用场景\n"
        "\n"
        "- macOS / Linux 用户（无 WPS COM 接口）\n"
        "- 无 WPS Office 的电脑（必须先安装 WPS）\n"
        "- 单文件大于 50 MB 的转换任务（需要修改源码再打包）\n"
        "- 需要把 PDF 反向转为 Word/Excel 的场景（本工具仅做正向转换）\n"
    )


# --------------------------------------------------------------------------- #
# 客户端：检查更新 & 下载
# --------------------------------------------------------------------------- #


@app.get("/update.json")
async def update_json(request: Request, db: AsyncSession = Depends(get_db)):
    """返回当前最新发布的元数据。Rust 客户端 check_update 调用这个。"""
    result = await db.execute(
        select(Release).where(Release.is_latest.is_(True)).limit(1)
    )
    release = result.scalar_one_or_none()
    if not release:
        raise HTTPException(status_code=404, detail="尚未发布任何版本")

    base = str(request.base_url).rstrip("/")
    return {
        "version": release.version,
        "url": f"{base}/downloads/{quote(release.filename, safe='')}",
        "sha256": release.sha256,
        "fileSize": release.file_size,
        "notes": release.notes,
        "pubDate": release.pub_date.isoformat(),
    }


@app.get("/downloads/{filename}")
async def download(
    filename: str,
    request: Request,
    db: AsyncSession = Depends(get_db),
):
    """下载安装包，支持 HTTP Range（断点续传）。"""
    # 防 path traversal
    safe = Path(filename).name
    if safe != filename:
        raise HTTPException(status_code=400, detail="非法文件名")

    file_path = STORAGE_DIR / safe
    if not file_path.is_file():
        raise HTTPException(status_code=404, detail="文件不存在")

    file_size = file_path.stat().st_size

    # 关联 release（用于日志中记录版本）
    versions = (await db.scalars(select(Release.version).where(Release.filename == filename).limit(2))).all()
    if not versions:
        raise HTTPException(status_code=404, detail="安装包尚未发布")
    if len(versions) != 1:
        raise HTTPException(status_code=409, detail="历史版本共用了文件名，请重新上传这些版本")
    version = versions[0]

    range_header = request.headers.get("range") or request.headers.get("Range")
    if range_header:
        match = RANGE_PATTERN.match(range_header.strip())
        if not match:
            raise HTTPException(status_code=400, detail="非法 Range 头")

        start_str, end_str = match.group(1), match.group(2)
        start = int(start_str) if start_str else 0
        end = int(end_str) if end_str else file_size - 1

        if start >= file_size or end >= file_size or start > end:
            return JSONResponse(
                status_code=416,
                content={"error": "Range Not Satisfiable"},
                headers={
                    "Content-Range": f"bytes */{file_size}",
                    "Accept-Ranges": "bytes",
                },
            )

        length = end - start + 1
        await _log_session(
            db, request, version, start, end, length, is_resume=start > 0
        )

        async def iter_chunked():
            with open(file_path, "rb") as fh:
                fh.seek(start)
                remaining = length
                while remaining > 0:
                    chunk = fh.read(min(CHUNK_SIZE, remaining))
                    if not chunk:
                        break
                    remaining -= len(chunk)
                    yield chunk

        headers = {
            "Content-Range": f"bytes {start}-{end}/{file_size}",
            "Accept-Ranges": "bytes",
            "Content-Length": str(length),
            "Content-Type": "application/octet-stream",
        }
        return StreamingResponse(iter_chunked(), status_code=206, headers=headers)

    # 全量下载
    await _log_session(db, request, version, 0, file_size - 1, file_size, is_resume=False)
    return FileResponse(
        file_path,
        media_type="application/octet-stream",
        filename=safe,
        headers={"Accept-Ranges": "bytes"},
    )


async def _log_session(
    db: AsyncSession,
    request: Request,
    version: str,
    start: int,
    end: int,
    length: int,
    is_resume: bool,
) -> None:
    session = DownloadSession(
        client_ip=request.client.host if request.client else None,
        version=version,
        range_start=start,
        range_end=end,
        bytes_sent=length,
        is_resume=is_resume,
        user_agent=request.headers.get("user-agent"),
    )
    db.add(session)
    await db.commit()


# --------------------------------------------------------------------------- #
# 管理端：发布 / 列表 / 会话
# --------------------------------------------------------------------------- #


async def lock_releases(db: AsyncSession) -> None:
    # Serialize publishers across workers so promotion and deletion see the same latest release.
    await db.execute(text("LOCK TABLE releases IN SHARE ROW EXCLUSIVE MODE"))


async def has_release(db: AsyncSession, filename: str) -> bool:
    return await db.scalar(select(Release.id).where(Release.filename == filename).limit(1)) is not None


@app.post("/admin/releases", dependencies=[Depends(require_admin)])
async def upload_release(
    version: str = Form(...),
    notes: Optional[str] = Form(None),
    is_latest: bool = Form(True),
    file: UploadFile = File(...),
    db: AsyncSession = Depends(get_db),
):
    """上传安装包并登记一条 release。流式落盘 + 计算 SHA256。"""
    if not file.filename:
        raise HTTPException(status_code=400, detail="缺少文件名")

    name = Path(file.filename).name
    extension = Path(name).suffix.lower()
    if extension not in {".exe", ".msi"}:
        raise HTTPException(status_code=400, detail="安装包须为 .exe 或 .msi")

    # Each upload owns its file, even when two versions use the same client filename.
    filename = f"AlexPDF-{uuid4().hex}{extension}"
    target = STORAGE_DIR / filename
    sha = hashlib.sha256()
    size = 0
    previous = None
    output = target.open("xb")
    try:
        with output as out:
            while chunk := await file.read(1024 * 1024):
                out.write(chunk)
                sha.update(chunk)
                size += len(chunk)
        if not size:
            raise HTTPException(status_code=400, detail="安装包为空")
        digest = sha.hexdigest()
        async with db.begin():
            await lock_releases(db)
            release = await db.scalar(select(Release).where(Release.version == version))
            if is_latest:
                await db.execute(update(Release).values(is_latest=False))
            if release is None:
                release = Release(version=version)
                db.add(release)
            else:
                previous = release.filename
            release.filename = filename
            release.sha256 = digest
            release.file_size = size
            release.notes = notes
            release.is_latest = is_latest
            await db.flush()
            release_id = release.id
            if previous and await has_release(db, previous):
                previous = None
    except BaseException:
        target.unlink(missing_ok=True)
        raise
    if previous:
        (STORAGE_DIR / previous).unlink(missing_ok=True)
    return {"ok": True, "id": release_id, "sha256": digest, "file_size": size}


@app.get("/admin/releases", dependencies=[Depends(require_admin)])
async def list_releases(db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Release).order_by(Release.pub_date.desc()))
    return [
        {
            "id": r.id,
            "version": r.version,
            "filename": r.filename,
            "sha256": r.sha256,
            "file_size": r.file_size,
            "is_latest": r.is_latest,
            "pub_date": r.pub_date.isoformat(),
        }
        for r in result.scalars()
    ]


@app.delete("/admin/releases/{release_id}", dependencies=[Depends(require_admin)])
async def delete_release(release_id: int, db: AsyncSession = Depends(get_db)):
    """删除一条 release。会顺手把 storage 里的安装包文件也删掉。"""
    await lock_releases(db)
    target = (
        await db.execute(select(Release).where(Release.id == release_id))
    ).scalar_one_or_none()
    if not target:
        raise HTTPException(status_code=404, detail="release 不存在")

    filename = target.filename
    was_latest = target.is_latest
    await db.delete(target)
    await db.flush()
    shared_file = await has_release(db, filename)

    # 如果删的是 latest，自动把最近发布的另一个标为 latest
    if was_latest:
        candidate = (
            await db.execute(
                select(Release).order_by(Release.pub_date.desc()).limit(1)
            )
        ).scalar_one_or_none()
        if candidate:
            candidate.is_latest = True
    await db.commit()
    if not shared_file:
        (STORAGE_DIR / filename).unlink(missing_ok=True)

    return {"ok": True}


@app.post("/admin/releases/{release_id}/promote", dependencies=[Depends(require_admin)])
async def promote_release(release_id: int, db: AsyncSession = Depends(get_db)):
    """把指定 release 设为 latest（其他自动取消标记）。"""
    await lock_releases(db)
    target = (
        await db.execute(select(Release).where(Release.id == release_id))
    ).scalar_one_or_none()
    if not target:
        raise HTTPException(status_code=404, detail="release 不存在")
    await db.execute(update(Release).values(is_latest=False))
    target.is_latest = True
    await db.commit()
    return {"ok": True, "version": target.version}


@app.get("/admin/sessions", dependencies=[Depends(require_admin)])
async def list_sessions(
    limit: int = Query(50, ge=1, le=500),
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(DownloadSession).order_by(DownloadSession.started_at.desc()).limit(limit)
    )
    return [
        {
            "id": s.id,
            "client_ip": s.client_ip,
            "version": s.version,
            "range_start": s.range_start,
            "range_end": s.range_end,
            "bytes_sent": s.bytes_sent,
            "is_resume": s.is_resume,
            "user_agent": s.user_agent,
            "started_at": s.started_at.isoformat(),
        }
        for s in result.scalars()
    ]


@app.get("/health")
async def health():
    return {"status": "ok"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        "main:app",
        host=os.getenv("HOST", "127.0.0.1"),
        port=int(os.getenv("PORT", "8800")),
        reload=False,
    )
