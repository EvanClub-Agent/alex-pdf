# AlexPDF 更新服务

可选的 FastAPI + PostgreSQL 服务，提供 Windows 安装包下载、更新清单和发布后台。转换文档只需桌面端。

## 本地运行

准备 PostgreSQL 数据库、能访问该库的账户、Python 3.11+ 和 uv。在仓库根目录执行以下命令。

```sh
createdb pdfupdates
cd update-server
uv sync --locked
cp .env.example .env
python -c "import secrets; print(secrets.token_urlsafe(32))"
```

在本机的 `.env` 填写实际 `DATABASE_URL` 和刚生成的 `ADMIN_TOKEN`。默认数据库地址使用本机 PostgreSQL 身份验证。Windows PowerShell 可用 `Copy-Item .env.example .env` 复制模板。

```sh
uv run --locked main.py
```

默认监听 `127.0.0.1:8800`。下载页位于 `/`，发布后台位于 `/admin`。后台会提示输入令牌。留空令牌时，公共下载端点仍可用，管理接口返回 503。Python 与 uv 仅用于运行此更新服务。

## 发布与更新

1. 在后台「上传新版本」填写与客户端一致的版本号，选择带版本号的安装包并勾选「设为最新」。服务端为每次上传分配独立文件名。
2. 访问 `/update.json` 核对版本、下载安装包的 URL 以及服务端计算的 SHA-256。
3. 启动 AlexPDF 前设置 `PDF_CONVERTER_UPDATE_URL` 指向该清单。本机测试可使用 `http://127.0.0.1:8800/update.json`。跨机器部署时使用 HTTPS 反向代理，并正确转发原始 Host 和协议，使清单包含有效的 HTTPS 下载地址。

| 方法 | 路径 | 用途 |
|---|---|---|
| GET | `/health` | 服务状态 |
| GET | `/update.json` | 当前最新发布版本的清单 |
| GET | `/downloads/{filename}` | 安装包，支持 HTTP Range 续传 |
| GET、POST | `/admin/releases` | 查看版本、上传安装包 |
| POST | `/admin/releases/{id}/promote` | 切换最新版本 |
| DELETE | `/admin/releases/{id}` | 删除版本及安装包 |
| GET | `/admin/sessions?limit=50` | 查看下载记录 |

脚本访问管理端点时使用 `Authorization: Bearer <token>`。安装包存放在 `STORAGE_DIR` 指定的目录中。数据库的下载记录包含请求 IP、User-Agent、版本、下载区间和请求时间。

项目采用 [MIT 许可证](../LICENSE)。
