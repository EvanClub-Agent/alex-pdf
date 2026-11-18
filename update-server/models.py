"""ORM 模型：发布版本 + 下载会话日志。"""
from __future__ import annotations

from datetime import datetime
from typing import Optional

from sqlalchemy import BigInteger, Boolean, DateTime, String, Text, func
from sqlalchemy.orm import Mapped, mapped_column

from database import Base


class Release(Base):
    __tablename__ = "releases"

    id: Mapped[int] = mapped_column(primary_key=True)
    version: Mapped[str] = mapped_column(String(64), unique=True, index=True)
    filename: Mapped[str] = mapped_column(String(256))
    sha256: Mapped[str] = mapped_column(String(64))
    file_size: Mapped[int] = mapped_column(BigInteger)
    notes: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    pub_date: Mapped[datetime] = mapped_column(
        DateTime(timezone=False), server_default=func.now()
    )
    is_latest: Mapped[bool] = mapped_column(Boolean, default=False, index=True)


class DownloadSession(Base):
    """每次 GET /downloads 请求都会落一行，包括首次和断点续传。"""

    __tablename__ = "download_sessions"

    id: Mapped[int] = mapped_column(primary_key=True)
    client_ip: Mapped[Optional[str]] = mapped_column(String(64), nullable=True)
    version: Mapped[str] = mapped_column(String(64), index=True)
    range_start: Mapped[int] = mapped_column(BigInteger, default=0)
    range_end: Mapped[Optional[int]] = mapped_column(BigInteger, nullable=True)
    bytes_sent: Mapped[int] = mapped_column(BigInteger, default=0)
    is_resume: Mapped[bool] = mapped_column(Boolean, default=False)
    user_agent: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    started_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=False), server_default=func.now()
    )
