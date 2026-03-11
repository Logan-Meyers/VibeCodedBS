use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;
use crate::api::Email;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch("
            CREATE TABLE IF NOT EXISTS emails (
                id          TEXT PRIMARY KEY,
                subject     TEXT,
                from_name   TEXT,
                from_addr   TEXT,
                preview     TEXT,
                body        TEXT,
                content_type TEXT,
                received_at TEXT,
                is_read     INTEGER NOT NULL DEFAULT 0,
                synced_at   TEXT NOT NULL DEFAULT (datetime('now'))
            );
        ")?;
        Ok(())
    }

    /// Upsert a list of emails into the cache
    pub fn upsert_emails(&self, emails: &[Email]) -> Result<()> {
        let mut stmt = self.conn.prepare("
            INSERT INTO emails (id, subject, from_name, from_addr, preview, body, content_type, received_at, is_read)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                subject = excluded.subject,
                from_name = excluded.from_name,
                from_addr = excluded.from_addr,
                preview = excluded.preview,
                body = excluded.body,
                content_type = excluded.content_type,
                received_at = excluded.received_at,
                is_read = excluded.is_read,
                synced_at = datetime('now')
        ")?;

        for email in emails {
            let (from_name, from_addr) = email.from.as_ref()
                .map(|f| (f.name.as_deref(), f.address.as_deref()))
                .unwrap_or((None, None));

            let (body, content_type) = email.body.as_ref()
                .map(|b| (Some(b.content.as_str()), Some(b.content_type.as_str())))
                .unwrap_or((None, None));

            stmt.execute(params![
                email.id,
                email.subject,
                from_name,
                from_addr,
                email.body_preview,
                body,
                content_type,
                email.received_at.map(|t| t.to_rfc3339()),
                email.is_read as i32,
            ])?;
        }
        Ok(())
    }

    /// List emails from cache, most recent first
    pub fn list_inbox(&self, limit: usize) -> Result<Vec<CachedEmail>> {
        let mut stmt = self.conn.prepare("
            SELECT id, subject, from_name, from_addr, preview, body, content_type, received_at, is_read
            FROM emails
            ORDER BY received_at DESC
            LIMIT ?1
        ")?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(CachedEmail {
                id: row.get(0)?,
                subject: row.get(1)?,
                from_name: row.get(2)?,
                from_addr: row.get(3)?,
                preview: row.get(4)?,
                body: row.get(5)?,
                content_type: row.get(6)?,
                received_at: row.get(7)?,
                is_read: row.get::<_, i32>(8)? != 0,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn mark_read(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE emails SET is_read = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn delete_email(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM emails WHERE id = ?1", params![id])?;
        Ok(())
    }
}

/// Flattened email row from SQLite cache
#[derive(Debug, Clone)]
pub struct CachedEmail {
    pub id: String,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_addr: Option<String>,
    pub preview: Option<String>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub received_at: Option<String>,
    pub is_read: bool,
}
