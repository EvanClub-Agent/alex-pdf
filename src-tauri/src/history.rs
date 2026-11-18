//! 任务历史持久化（SQLite）。
//!
//! 表结构：
//! ```sql
//! CREATE TABLE tasks (
//!   task_id    TEXT PRIMARY KEY,
//!   status     TEXT NOT NULL,
//!   started_at TEXT NOT NULL,
//!   payload    TEXT NOT NULL    -- 完整序列化的 TaskStatusResponse JSON
//! );
//! ```
//!
//! 把整个响应作为 JSON blob 落库，schema 简单，且新字段加进 TaskStatusResponse
//! 后历史不会失效；status / started_at 单独抽出来用于过滤与排序。
//!
//! 启动时若发现旧版的 history.jsonl，做一次性导入并重命名为 history.jsonl.imported。

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use tracing::warn;

use crate::state::TaskStatusResponse;

const SQLITE_FILE: &str = "history.sqlite";
const LEGACY_JSONL_FILE: &str = "history.jsonl";
const MAX_RETURN: usize = 500;

pub struct HistoryStore {
  conn: Option<Mutex<Connection>>,
}

impl HistoryStore {
  pub fn new() -> Self {
    let Some(path) = data_dir().map(|d| d.join(SQLITE_FILE)) else {
      return Self { conn: None };
    };
    if let Some(parent) = path.parent() {
      let _ = std::fs::create_dir_all(parent);
    }
    let conn = match Connection::open(&path) {
      Ok(c) => c,
      Err(err) => {
        warn!("history open failed: {}", err);
        return Self { conn: None };
      }
    };
    if let Err(err) = init_schema(&conn) {
      warn!("history schema init failed: {}", err);
      return Self { conn: None };
    }

    if let Some(parent) = path.parent() {
      let legacy = parent.join(LEGACY_JSONL_FILE);
      if legacy.is_file() {
        match import_jsonl(&conn, &legacy) {
          Ok(count) => {
            tracing::info!("history: 导入 {} 条遗留 jsonl 记录", count);
            let archived = parent.join(format!("{}.imported", LEGACY_JSONL_FILE));
            let _ = std::fs::rename(&legacy, archived);
          }
          Err(err) => warn!("legacy jsonl import failed: {}", err),
        }
      }
    }

    Self {
      conn: Some(Mutex::new(conn)),
    }
  }

  pub async fn append(&self, entry: &TaskStatusResponse) {
    let Some(conn) = self.conn.as_ref() else {
      return;
    };
    let payload = match serde_json::to_string(entry) {
      Ok(s) => s,
      Err(err) => {
        warn!("history serialize failed: {}", err);
        return;
      }
    };
    let conn = conn.lock().unwrap();
    let result = conn.execute(
      "INSERT OR REPLACE INTO tasks (task_id, status, started_at, payload)
       VALUES (?1, ?2, ?3, ?4)",
      params![
        entry.task_id,
        entry.status.as_str(),
        entry.started_at,
        payload
      ],
    );
    if let Err(err) = result {
      warn!("history append failed: {}", err);
    }
  }

  pub async fn recent(
    &self,
    limit: usize,
    status_filter: Option<&str>,
  ) -> Vec<TaskStatusResponse> {
    let Some(conn) = self.conn.as_ref() else {
      return Vec::new();
    };
    let cap = limit.min(MAX_RETURN) as i64;
    let conn = conn.lock().unwrap();

    let mut entries = Vec::new();
    let exec = (|| -> rusqlite::Result<()> {
      if let Some(filter) = status_filter {
        let mut stmt = conn.prepare(
          "SELECT payload FROM tasks WHERE status = ?1 ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![filter, cap], |row| row.get::<_, String>(0))?;
        for payload in rows.flatten() {
          if let Ok(item) = serde_json::from_str::<TaskStatusResponse>(&payload) {
            entries.push(item);
          }
        }
      } else {
        let mut stmt = conn
          .prepare("SELECT payload FROM tasks ORDER BY started_at DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![cap], |row| row.get::<_, String>(0))?;
        for payload in rows.flatten() {
          if let Ok(item) = serde_json::from_str::<TaskStatusResponse>(&payload) {
            entries.push(item);
          }
        }
      }
      Ok(())
    })();
    if let Err(err) = exec {
      warn!("history query failed: {}", err);
    }
    entries
  }
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
  conn.execute_batch(
    "CREATE TABLE IF NOT EXISTS tasks (
       task_id    TEXT PRIMARY KEY,
       status     TEXT NOT NULL,
       started_at TEXT NOT NULL,
       payload    TEXT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
     CREATE INDEX IF NOT EXISTS idx_tasks_started_at ON tasks(started_at);",
  )
}

fn import_jsonl(conn: &Connection, path: &std::path::Path) -> rusqlite::Result<usize> {
  let content = std::fs::read_to_string(path)
    .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
  let tx = conn.unchecked_transaction()?;
  let mut count = 0;
  for line in content.lines() {
    let trimmed = line.trim();
    if trimmed.is_empty() {
      continue;
    }
    let Ok(entry) = serde_json::from_str::<TaskStatusResponse>(trimmed) else {
      continue;
    };
    tx.execute(
      "INSERT OR REPLACE INTO tasks (task_id, status, started_at, payload)
       VALUES (?1, ?2, ?3, ?4)",
      params![
        entry.task_id,
        entry.status.as_str(),
        entry.started_at,
        trimmed
      ],
    )?;
    count += 1;
  }
  tx.commit()?;
  Ok(count)
}

fn data_dir() -> Option<PathBuf> {
  if !cfg!(target_os = "windows") {
    if let Some(home) = std::env::var_os("HOME") {
      return Some(PathBuf::from(home).join(".pdf-converter"));
    }
  }
  let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("LOCALAPPDATA"))?;
  Some(PathBuf::from(base).join("com.wordtopdf.converter"))
}
