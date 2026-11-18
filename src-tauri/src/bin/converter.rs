//! `converter` CLI —— 通过本地 HTTP API 调用桌面服务。
//!
//! 用例：
//!   converter convert input.docx -o ./pdfs
//!   converter url https://example.com/a.docx
//!   converter batch-url https://a.com/x.docx https://a.com/y.docx
//!   converter status <task_id>
//!   converter cancel <task_id>
//!   converter tasks --limit 20
//!   converter diagnostics
//!
//! Token：优先读环境变量 PDF_CONVERTER_API_TOKEN，否则尝试读
//! `%APPDATA%/com.wordtopdf.converter/desktop_settings.json` 中的 api_token。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use reqwest::blocking::{multipart, Client};
use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:24568";
const TOKEN_ENV: &str = "PDF_CONVERTER_API_TOKEN";

#[derive(Parser)]
#[command(name = "converter", version, about = "AlexPDF CLI")]
struct Cli {
  /// 服务地址（默认 http://127.0.0.1:24568）
  #[arg(long, global = true)]
  base_url: Option<String>,
  /// API token，覆盖环境变量与配置文件
  #[arg(long, global = true)]
  token: Option<String>,
  /// 输出原始 JSON 响应而非人类可读摘要
  #[arg(long, global = true)]
  json: bool,

  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// 上传单个文件转换
  Convert {
    file: PathBuf,
    /// 输出目录（不传则用服务端默认）
    #[arg(short = 'o', long)]
    output_dir: Option<PathBuf>,
    /// 自定义输出文件名
    #[arg(long)]
    name: Option<String>,
    /// 引擎：auto / wps / office
    #[arg(long)]
    engine: Option<String>,
  },
  /// 批量上传文件转换
  Batch {
    files: Vec<PathBuf>,
    #[arg(short = 'o', long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    engine: Option<String>,
  },
  /// 通过 URL 转换
  Url {
    url: String,
    #[arg(short = 'o', long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    name: Option<String>,
  },
  /// 通过 URL 批量转换
  BatchUrl {
    urls: Vec<String>,
    #[arg(short = 'o', long)]
    output_dir: Option<PathBuf>,
  },
  /// 查任务状态
  Status { task_id: String },
  /// 取消任务
  Cancel { task_id: String },
  /// 列出最近任务
  Tasks {
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    status: Option<String>,
  },
  /// 服务诊断
  Diagnostics,
}

fn main() -> ExitCode {
  let cli = Cli::parse();
  match run(&cli) {
    Ok(()) => ExitCode::SUCCESS,
    Err(err) => {
      eprintln!("error: {}", err);
      ExitCode::FAILURE
    }
  }
}

fn run(cli: &Cli) -> anyhow::Result<()> {
  let base_url = cli
    .base_url
    .clone()
    .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
  let token = resolve_token(cli.token.as_deref());
  let client = build_client(&token)?;

  match &cli.command {
    Command::Convert { file, output_dir, name, engine } => {
      cmd_convert(&client, &base_url, file, output_dir.as_deref(), name.as_deref(), engine.as_deref(), cli.json)
    }
    Command::Batch { files, output_dir, engine } => {
      cmd_batch(&client, &base_url, files, output_dir.as_deref(), engine.as_deref(), cli.json)
    }
    Command::Url { url, output_dir, name } => {
      cmd_url(&client, &base_url, url, output_dir.as_deref(), name.as_deref(), cli.json)
    }
    Command::BatchUrl { urls, output_dir } => {
      cmd_batch_url(&client, &base_url, urls, output_dir.as_deref(), cli.json)
    }
    Command::Status { task_id } => cmd_status(&client, &base_url, task_id, cli.json),
    Command::Cancel { task_id } => cmd_cancel(&client, &base_url, task_id, cli.json),
    Command::Tasks { limit, status } => cmd_tasks(&client, &base_url, *limit, status.as_deref(), cli.json),
    Command::Diagnostics => cmd_diagnostics(&client, &base_url, cli.json),
  }
}

fn build_client(token: &Option<String>) -> anyhow::Result<Client> {
  let mut headers = reqwest::header::HeaderMap::new();
  if let Some(token) = token {
    headers.insert(
      reqwest::header::AUTHORIZATION,
      format!("Bearer {}", token).parse()?,
    );
  }
  Ok(Client::builder().default_headers(headers).build()?)
}

fn resolve_token(cli_token: Option<&str>) -> Option<String> {
  if let Some(t) = cli_token {
    return Some(t.to_string());
  }
  if let Ok(t) = std::env::var(TOKEN_ENV) {
    if !t.is_empty() {
      return Some(t);
    }
  }
  read_token_from_config()
}

fn read_token_from_config() -> Option<String> {
  #[derive(Deserialize)]
  struct DesktopSettings {
    api_token: Option<String>,
  }
  let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("LOCALAPPDATA"))?;
  let path = PathBuf::from(base)
    .join("com.wordtopdf.converter")
    .join("desktop_settings.json");
  let text = std::fs::read_to_string(path).ok()?;
  serde_json::from_str::<DesktopSettings>(&text)
    .ok()?
    .api_token
}

fn cmd_convert(
  client: &Client,
  base_url: &str,
  file: &Path,
  output_dir: Option<&Path>,
  name: Option<&str>,
  engine: Option<&str>,
  json_mode: bool,
) -> anyhow::Result<()> {
  // multipart：file 字段 + output_name；output_dir 通过 query 不可，需走 server 端默认或先
  // 设定全局默认。这里只支持服务端默认 / 自定义文件名。
  let _ = output_dir; // 保留参数以备将来用 (multipart 没法传 output_folder 给单文件路径)
  let mut form = multipart::Form::new();
  form = form.file("file", file)?;
  if let Some(n) = name {
    form = form.text("output_name", n.to_string());
  }
  let mut url = format!("{}/api/convert?async=false", base_url);
  if let Some(e) = engine {
    url.push_str(&format!("&engine={}", e));
  }
  let resp = client.post(url).multipart(form).send()?.error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn cmd_batch(
  client: &Client,
  base_url: &str,
  files: &[PathBuf],
  output_dir: Option<&Path>,
  engine: Option<&str>,
  json_mode: bool,
) -> anyhow::Result<()> {
  let mut form = multipart::Form::new();
  for f in files {
    form = form.file("files", f)?;
  }
  if let Some(d) = output_dir {
    form = form.text("output_folder", d.display().to_string());
  }
  let mut url = format!("{}/api/convert-batch?async=false", base_url);
  if let Some(e) = engine {
    url.push_str(&format!("&engine={}", e));
  }
  let resp = client.post(url).multipart(form).send()?.error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn cmd_url(
  client: &Client,
  base_url: &str,
  url: &str,
  output_dir: Option<&Path>,
  name: Option<&str>,
  json_mode: bool,
) -> anyhow::Result<()> {
  let body = serde_json::json!({
    "url": url,
    "output_name": name,
    "output_folder": output_dir.map(|p| p.display().to_string()),
  });
  let resp = client
    .post(format!("{}/api/convert-from-url?async=false", base_url))
    .json(&body)
    .send()?
    .error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn cmd_batch_url(
  client: &Client,
  base_url: &str,
  urls: &[String],
  output_dir: Option<&Path>,
  json_mode: bool,
) -> anyhow::Result<()> {
  let body = serde_json::json!({
    "urls": urls,
    "output_folder": output_dir.map(|p| p.display().to_string()),
  });
  let resp = client
    .post(format!("{}/api/convert-batch-from-urls?async=false", base_url))
    .json(&body)
    .send()?
    .error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn cmd_status(client: &Client, base_url: &str, task_id: &str, json_mode: bool) -> anyhow::Result<()> {
  let resp = client
    .get(format!("{}/api/status/{}", base_url, task_id))
    .send()?
    .error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn cmd_cancel(client: &Client, base_url: &str, task_id: &str, json_mode: bool) -> anyhow::Result<()> {
  let resp = client
    .post(format!("{}/api/cancel/{}", base_url, task_id))
    .send()?
    .error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn cmd_tasks(
  client: &Client,
  base_url: &str,
  limit: Option<usize>,
  status: Option<&str>,
  json_mode: bool,
) -> anyhow::Result<()> {
  let mut url = format!("{}/api/tasks", base_url);
  let mut params = Vec::new();
  if let Some(l) = limit {
    params.push(format!("limit={}", l));
  }
  if let Some(s) = status {
    params.push(format!("status={}", s));
  }
  if !params.is_empty() {
    url.push('?');
    url.push_str(&params.join("&"));
  }
  let resp = client.get(url).send()?.error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn cmd_diagnostics(client: &Client, base_url: &str, json_mode: bool) -> anyhow::Result<()> {
  let resp = client
    .get(format!("{}/api/diagnostics", base_url))
    .send()?
    .error_for_status()?;
  emit(json_mode, resp.text()?)
}

fn emit(json_mode: bool, body: String) -> anyhow::Result<()> {
  if json_mode {
    println!("{}", body);
  } else {
    // 尝试 pretty print，失败则原样输出。
    match serde_json::from_str::<serde_json::Value>(&body) {
      Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
      Err(_) => println!("{}", body),
    }
  }
  Ok(())
}
