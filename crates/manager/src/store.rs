use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::{DateTime, Utc};
use domain::{EnvironmentRecord, HostEvent, ManagerEvent, ManagerSettings, OperationRecord};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite operation failed: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("stored JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("stored timestamp is invalid: {0}")]
    Time(#[from] chrono::ParseError),
    #[error("SQLite connection lock is poisoned")]
    Poisoned,
    #[error("invalid operation transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

#[derive(Clone)]
pub struct ManagerStore {
    path: Arc<PathBuf>,
    connection: Arc<Mutex<Connection>>,
}

pub struct RuntimeUpdate<'a> {
    pub env_id: &'a str,
    pub generation: u64,
    pub status: &'a str,
    pub request_id: Option<i32>,
    pub operation_id: Option<&'a str>,
    pub cdp: &'a str,
    pub last_event: &'a str,
}

impl ManagerStore {
    pub fn open_default() -> Result<Self, StoreError> {
        let data_dir = platform::default_data_dir();
        let defaults = ManagerSettings {
            work_dir: platform::default_sdk_work_dir().display().to_string(),
            extension_dir: platform::default_extension_dir().display().to_string(),
            log_dir: platform::default_log_dir().display().to_string(),
            sdk_api_url: None,
            debug: false,
        };
        Self::open(data_dir.join("manager.sqlite3"), &defaults)
    }

    pub fn open(path: impl AsRef<Path>, defaults: &ManagerSettings) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
            })?;
        }
        let connection = Connection::open(&path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                work_dir TEXT NOT NULL,
                extension_dir TEXT NOT NULL,
                log_dir TEXT NOT NULL,
                sdk_api_url TEXT,
                debug INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS environments (
                env_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                local_label TEXT NOT NULL DEFAULT '',
                tags_json TEXT NOT NULL DEFAULT '[]',
                remote_json TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL DEFAULT 'stopped',
                cdp TEXT NOT NULL DEFAULT '-',
                last_event TEXT NOT NULL DEFAULT '',
                generation INTEGER NOT NULL DEFAULT 0,
                request_id INTEGER,
                current_operation_id TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS operations (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                env_id TEXT,
                label TEXT NOT NULL,
                status TEXT NOT NULL,
                message TEXT NOT NULL DEFAULT '',
                request_id INTEGER,
                generation INTEGER NOT NULL DEFAULT 0,
                error_code TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS operations_status_idx ON operations(status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS operations_env_idx ON operations(env_id, updated_at DESC);

            CREATE TABLE IF NOT EXISTS runtime_snapshots (
                env_id TEXT PRIMARY KEY,
                generation INTEGER NOT NULL,
                request_id INTEGER,
                state TEXT NOT NULL,
                cdp TEXT NOT NULL DEFAULT '-',
                last_event TEXT NOT NULL DEFAULT '',
                observed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS proxy_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                scheme TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                username TEXT,
                secret_ref TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS fingerprint_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                profile_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS manager_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                env_id TEXT,
                operation_id TEXT,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )?;
        let now = timestamp();
        connection.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
            params![SCHEMA_VERSION, now],
        )?;
        connection.execute(
            r#"INSERT OR IGNORE INTO settings(
                id, work_dir, extension_dir, log_dir, sdk_api_url, debug, updated_at
            ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)"#,
            params![
                defaults.work_dir,
                defaults.extension_dir,
                defaults.log_dir,
                defaults.sdk_api_url,
                defaults.debug,
                now,
            ],
        )?;
        Ok(Self {
            path: Arc::new(path),
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn settings(&self) -> Result<ManagerSettings, StoreError> {
        self.connection()?.query_row(
            "SELECT work_dir, extension_dir, log_dir, sdk_api_url, debug FROM settings WHERE id = 1",
            [],
            |row| {
                Ok(ManagerSettings {
                    work_dir: row.get(0)?,
                    extension_dir: row.get(1)?,
                    log_dir: row.get(2)?,
                    sdk_api_url: row.get(3)?,
                    debug: row.get(4)?,
                })
            },
        ).map_err(Into::into)
    }

    pub fn update_settings(&self, settings: &ManagerSettings) -> Result<(), StoreError> {
        self.connection()?.execute(
            r#"UPDATE settings SET
                work_dir = ?1, extension_dir = ?2, log_dir = ?3,
                sdk_api_url = ?4, debug = ?5, updated_at = ?6
            WHERE id = 1"#,
            params![
                settings.work_dir,
                settings.extension_dir,
                settings.log_dir,
                settings.sdk_api_url,
                settings.debug,
                timestamp(),
            ],
        )?;
        Ok(())
    }

    pub fn list_environments(&self) -> Result<Vec<EnvironmentRecord>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT env_id, name, local_label, tags_json, status, cdp, last_event,
                      generation, request_id, current_operation_id, updated_at
               FROM environments ORDER BY COALESCE(NULLIF(local_label, ''), name), env_id"#,
        )?;
        let rows = statement.query_map([], environment_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn upsert_remote_environments(
        &self,
        environments: &[(String, String, Value)],
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let now = timestamp();
        for (env_id, name, remote) in environments {
            transaction.execute(
                r#"INSERT INTO environments(
                    env_id, name, remote_json, status, cdp, last_event, updated_at
                ) VALUES (?1, ?2, ?3, 'stopped', '-', 'env_page sync', ?4)
                ON CONFLICT(env_id) DO UPDATE SET
                    name = excluded.name,
                    remote_json = excluded.remote_json,
                    updated_at = excluded.updated_at"#,
                params![env_id, name, serde_json::to_string(remote)?, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn create_operation(
        &self,
        kind: &str,
        env_id: Option<&str>,
        label: &str,
        generation: u64,
    ) -> Result<OperationRecord, StoreError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO operations(
                id, kind, env_id, label, status, message, generation, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, 'queued', 'queued', ?5, ?6, ?6)"#,
            params![id, kind, env_id, label, generation, now.to_rfc3339()],
        )?;
        append_event_tx(
            &transaction,
            "operation.queued",
            env_id,
            Some(&id),
            &json!({
                "kind": kind,
                "status": "queued",
                "generation": generation,
            }),
        )?;
        transaction.commit()?;
        drop(connection);
        self.operation(&id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn operation(&self, id: &str) -> Result<Option<OperationRecord>, StoreError> {
        self.connection()?
            .query_row(
                r#"SELECT id, kind, env_id, label, status, message, request_id,
                          generation, error_code, created_at, updated_at
                   FROM operations WHERE id = ?1"#,
                [id],
                operation_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_operations(&self, limit: usize) -> Result<Vec<OperationRecord>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT id, kind, env_id, label, status, message, request_id,
                      generation, error_code, created_at, updated_at
               FROM operations ORDER BY updated_at DESC LIMIT ?1"#,
        )?;
        let rows = statement.query_map([limit as i64], operation_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn transition_operation(
        &self,
        id: &str,
        status: &str,
        message: &str,
        error_code: Option<&str>,
    ) -> Result<OperationRecord, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let current =
            operation_tx(&transaction, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if !valid_transition(&current.status, status) {
            return Err(StoreError::InvalidTransition {
                from: current.status,
                to: status.into(),
            });
        }
        transaction.execute(
            r#"UPDATE operations SET status = ?1, message = ?2, error_code = ?3,
                      updated_at = ?4 WHERE id = ?5"#,
            params![status, message, error_code, timestamp(), id],
        )?;
        append_event_tx(
            &transaction,
            &format!("operation.{status}"),
            current.env_id.as_deref(),
            Some(id),
            &json!({
                "kind": current.kind,
                "status": status,
                "message": message,
                "errorCode": error_code,
                "generation": current.generation,
            }),
        )?;
        transaction.commit()?;
        drop(connection);
        self.operation(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn set_operation_request_id(&self, id: &str, request_id: i32) -> Result<(), StoreError> {
        self.connection()?.execute(
            "UPDATE operations SET request_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![request_id, timestamp(), id],
        )?;
        Ok(())
    }

    pub fn next_generation(&self, env_id: &str) -> Result<u64, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let current: i64 = transaction.query_row(
            "SELECT generation FROM environments WHERE env_id = ?1",
            [env_id],
            |row| row.get(0),
        )?;
        let next = current + 1;
        transaction.execute(
            "UPDATE environments SET generation = ?1, updated_at = ?2 WHERE env_id = ?3",
            params![next, timestamp(), env_id],
        )?;
        transaction.commit()?;
        Ok(next as u64)
    }

    pub fn set_environment_runtime(&self, update: RuntimeUpdate<'_>) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"UPDATE environments SET status = ?1, request_id = ?2,
                      current_operation_id = ?3, cdp = ?4, last_event = ?5,
                      updated_at = ?6
               WHERE env_id = ?7 AND generation = ?8"#,
            params![
                update.status,
                update.request_id,
                update.operation_id,
                update.cdp,
                update.last_event,
                timestamp(),
                update.env_id,
                update.generation,
            ],
        )?;
        transaction.execute(
            r#"INSERT INTO runtime_snapshots(
                env_id, generation, request_id, state, cdp, last_event, observed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(env_id) DO UPDATE SET
                generation = excluded.generation,
                request_id = excluded.request_id,
                state = excluded.state,
                cdp = excluded.cdp,
                last_event = excluded.last_event,
                observed_at = excluded.observed_at"#,
            params![
                update.env_id,
                update.generation,
                update.request_id,
                update.status,
                update.cdp,
                update.last_event,
                timestamp(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn append_event(
        &self,
        event_type: &str,
        env_id: Option<&str>,
        operation_id: Option<&str>,
        payload: &Value,
    ) -> Result<ManagerEvent, StoreError> {
        let connection = self.connection()?;
        let now = Utc::now();
        connection.execute(
            r#"INSERT INTO manager_events(
                event_type, env_id, operation_id, payload_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)"#,
            params![
                event_type,
                env_id,
                operation_id,
                serde_json::to_string(payload)?,
                now.to_rfc3339(),
            ],
        )?;
        Ok(ManagerEvent {
            sequence: connection.last_insert_rowid() as u64,
            event_type: event_type.into(),
            env_id: env_id.map(str::to_string),
            operation_id: operation_id.map(str::to_string),
            payload: payload.clone(),
            created_at: now,
        })
    }

    pub fn events_since(
        &self,
        sequence: u64,
        limit: usize,
    ) -> Result<Vec<ManagerEvent>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT sequence, event_type, env_id, operation_id, payload_json, created_at
               FROM manager_events WHERE sequence > ?1 ORDER BY sequence LIMIT ?2"#,
        )?;
        let rows = statement.query_map(params![sequence, limit as i64], |row| {
            let payload: String = row.get(4)?;
            let created_at: String = row.get(5)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                payload,
                created_at,
            ))
        })?;
        rows.map(|row| {
            let (sequence, event_type, env_id, operation_id, payload, created_at) = row?;
            Ok(ManagerEvent {
                sequence: sequence as u64,
                event_type,
                env_id,
                operation_id,
                payload: serde_json::from_str(&payload)?,
                created_at: parse_time(&created_at)?,
            })
        })
        .collect()
    }

    pub fn latest_event_sequence(&self) -> Result<u64, StoreError> {
        self.connection()?
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) FROM manager_events",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value as u64)
            .map_err(Into::into)
    }

    pub fn apply_host_event(&self, event: &HostEvent) -> Result<ManagerEvent, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let payload = json!({
            "hostSequence": event.sequence,
            "eventName": event.event_name,
            "code": event.code,
            "requestId": event.request_id,
            "payload": event.payload,
        });
        let manager_event = append_event_tx(
            &transaction,
            &event.event_type,
            event.env_id.as_deref(),
            event.operation_id.as_deref(),
            &payload,
        )?;

        if let Some(operation_id) = event.operation_id.as_deref()
            && let Some(operation) = operation_tx(&transaction, operation_id)?
            && matches!(operation.status.as_str(), "queued" | "running")
            && let Some(env_id) = event.env_id.as_deref().or(operation.env_id.as_deref())
        {
            let current_generation: Option<i64> = transaction
                .query_row(
                    "SELECT generation FROM environments WHERE env_id = ?1",
                    [env_id],
                    |row| row.get(0),
                )
                .optional()?;
            if current_generation == Some(operation.generation as i64) {
                apply_lifecycle_event(&transaction, env_id, &operation, event)?;
            }
        }

        transaction.commit()?;
        Ok(manager_event)
    }

    pub fn mark_host_degraded(&self, message: &str) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let now = timestamp();
        transaction.execute(
            r#"UPDATE environments SET status = 'unknown', last_event = ?1,
                      current_operation_id = NULL, updated_at = ?2
               WHERE status IN ('preparing', 'starting', 'ready', 'stopping')"#,
            params![message, now],
        )?;
        transaction.execute(
            r#"UPDATE operations SET status = 'failed', message = ?1,
                      error_code = 'HOST_DEGRADED', updated_at = ?2
               WHERE status IN ('queued', 'running')"#,
            params![message, now],
        )?;
        append_event_tx(
            &transaction,
            "runtime.degraded",
            None,
            None,
            &json!({ "message": message }),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reconcile_running_environments(
        &self,
        running: &HashMap<String, String>,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let mut statement = transaction.prepare(
            "SELECT env_id, status FROM environments WHERE status IN ('starting', 'ready', 'stopping', 'unknown')",
        )?;
        let candidates = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for (env_id, previous) in candidates {
            let (status, cdp, event) = match running.get(&env_id) {
                Some(cdp) => (
                    "ready",
                    cdp.as_str(),
                    "sdk_browser_info reconciliation: running",
                ),
                None => (
                    "stopped",
                    "-",
                    "sdk_browser_info reconciliation: not running",
                ),
            };
            if previous != status {
                transaction.execute(
                    r#"UPDATE environments SET status = ?1, cdp = ?2, last_event = ?3,
                              current_operation_id = NULL, updated_at = ?4 WHERE env_id = ?5"#,
                    params![status, cdp, event, timestamp(), env_id],
                )?;
                append_event_tx(
                    &transaction,
                    "runtime.reconciled",
                    Some(&env_id),
                    None,
                    &json!({ "previous": previous, "status": status, "cdp": cdp }),
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::Poisoned)
    }
}

fn append_event_tx(
    transaction: &Transaction<'_>,
    event_type: &str,
    env_id: Option<&str>,
    operation_id: Option<&str>,
    payload: &Value,
) -> Result<ManagerEvent, StoreError> {
    let now = Utc::now();
    transaction.execute(
        r#"INSERT INTO manager_events(
            event_type, env_id, operation_id, payload_json, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)"#,
        params![
            event_type,
            env_id,
            operation_id,
            serde_json::to_string(payload)?,
            now.to_rfc3339(),
        ],
    )?;
    Ok(ManagerEvent {
        sequence: transaction.last_insert_rowid() as u64,
        event_type: event_type.into(),
        env_id: env_id.map(str::to_string),
        operation_id: operation_id.map(str::to_string),
        payload: payload.clone(),
        created_at: now,
    })
}

fn apply_lifecycle_event(
    transaction: &Transaction<'_>,
    env_id: &str,
    operation: &OperationRecord,
    event: &HostEvent,
) -> Result<(), StoreError> {
    let event_name = event.event_name.to_ascii_lowercase();
    let (operation_status, environment_status) = if operation.kind == "environment.start"
        && event_name.contains("browser-open-success")
    {
        ("succeeded", "ready")
    } else if operation.kind == "environment.stop" && event_name.contains("browser-close-success") {
        ("succeeded", "stopped")
    } else if event_name.contains("fail") || event_name.contains("error") || event.code < 0 {
        ("failed", "failed")
    } else {
        return Ok(());
    };
    let now = timestamp();
    transaction.execute(
        r#"UPDATE operations SET status = ?1, message = ?2,
                  error_code = CASE WHEN ?1 = 'failed' THEN 'SDK_EVENT_FAILED' ELSE NULL END,
                  updated_at = ?3 WHERE id = ?4"#,
        params![operation_status, event.event_name, now, operation.id],
    )?;
    transaction.execute(
        r#"UPDATE environments SET status = ?1, last_event = ?2,
                  current_operation_id = NULL, updated_at = ?3
           WHERE env_id = ?4 AND generation = ?5"#,
        params![
            environment_status,
            event.event_name,
            now,
            env_id,
            operation.generation,
        ],
    )?;
    Ok(())
}

fn operation_tx(
    transaction: &Transaction<'_>,
    id: &str,
) -> Result<Option<OperationRecord>, StoreError> {
    transaction
        .query_row(
            r#"SELECT id, kind, env_id, label, status, message, request_id,
                      generation, error_code, created_at, updated_at
               FROM operations WHERE id = ?1"#,
            [id],
            operation_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn environment_from_row(row: &Row<'_>) -> rusqlite::Result<EnvironmentRecord> {
    let tags: String = row.get(3)?;
    let updated_at: String = row.get(10)?;
    Ok(EnvironmentRecord {
        env_id: row.get(0)?,
        name: row.get(1)?,
        local_label: row.get(2)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        status: row.get(4)?,
        cdp: row.get(5)?,
        last_event: row.get(6)?,
        generation: row.get::<_, i64>(7)? as u64,
        request_id: row.get(8)?,
        current_operation_id: row.get(9)?,
        updated_at: parse_time(&updated_at).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

fn operation_from_row(row: &Row<'_>) -> rusqlite::Result<OperationRecord> {
    let created_at: String = row.get(9)?;
    let updated_at: String = row.get(10)?;
    Ok(OperationRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        env_id: row.get(2)?,
        label: row.get(3)?,
        status: row.get(4)?,
        message: row.get(5)?,
        request_id: row.get(6)?,
        generation: row.get::<_, i64>(7)? as u64,
        error_code: row.get(8)?,
        created_at: parse_time(&created_at).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                9,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        updated_at: parse_time(&updated_at).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

fn valid_transition(from: &str, to: &str) -> bool {
    from == to
        || matches!(
            (from, to),
            ("queued", "running" | "cancelled" | "failed")
                | ("running", "succeeded" | "failed" | "cancelled")
        )
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(directory: &tempfile::TempDir) -> ManagerStore {
        ManagerStore::open(
            directory.path().join("manager.sqlite3"),
            &ManagerSettings {
                work_dir: "work".into(),
                extension_dir: "extensions".into(),
                log_dir: "logs".into(),
                sdk_api_url: None,
                debug: false,
            },
        )
        .expect("open store")
    }

    #[test]
    fn settings_and_operations_survive_reopen() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("manager.sqlite3");
        let store = test_store(&directory);
        let mut settings = store.settings().expect("settings");
        settings.debug = true;
        store.update_settings(&settings).expect("update settings");
        let operation = store
            .create_operation("environment.sync", None, "同步环境", 0)
            .expect("create operation");
        store
            .transition_operation(&operation.id, "running", "running", None)
            .expect("start operation");
        drop(store);

        let reopened = ManagerStore::open(
            path,
            &ManagerSettings {
                work_dir: "unused".into(),
                extension_dir: "unused".into(),
                log_dir: "unused".into(),
                sdk_api_url: None,
                debug: false,
            },
        )
        .expect("reopen store");
        assert!(reopened.settings().expect("settings").debug);
        assert_eq!(
            reopened
                .operation(&operation.id)
                .expect("operation")
                .expect("stored operation")
                .status,
            "running"
        );
    }

    #[test]
    fn late_open_event_does_not_revive_new_generation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        store
            .upsert_remote_environments(&[(
                "env-1".into(),
                "Environment".into(),
                json!({ "envId": "env-1" }),
            )])
            .expect("upsert environment");
        let generation = store.next_generation("env-1").expect("generation 1");
        let operation = store
            .create_operation("environment.start", Some("env-1"), "启动环境", generation)
            .expect("operation");
        store
            .transition_operation(&operation.id, "running", "starting", None)
            .expect("start operation");
        store.next_generation("env-1").expect("generation 2");
        store
            .set_environment_runtime(RuntimeUpdate {
                env_id: "env-1",
                generation: 2,
                status: "stopped",
                request_id: None,
                operation_id: None,
                cdp: "-",
                last_event: "stopped",
            })
            .expect("stop newer generation");

        store
            .apply_host_event(&HostEvent {
                sequence: 1,
                event_type: "sdk.result".into(),
                code: 0,
                event_name: "browser-open-success".into(),
                request_id: Some(42),
                operation_id: Some(operation.id),
                env_id: Some("env-1".into()),
                payload: json!({}),
                received_at: Utc::now(),
            })
            .expect("apply late event");
        let environment = store.list_environments().expect("environments").remove(0);
        assert_eq!(environment.generation, 2);
        assert_eq!(environment.status, "stopped");
    }

    #[test]
    fn reconciliation_detects_manual_close() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        store
            .upsert_remote_environments(&[(
                "env-1".into(),
                "Environment".into(),
                json!({ "envId": "env-1" }),
            )])
            .expect("upsert environment");
        let generation = store.next_generation("env-1").expect("generation");
        store
            .set_environment_runtime(RuntimeUpdate {
                env_id: "env-1",
                generation,
                status: "ready",
                request_id: Some(7),
                operation_id: None,
                cdp: "ws://127.0.0.1/devtools/browser/1",
                last_event: "ready",
            })
            .expect("mark ready");
        store
            .reconcile_running_environments(&HashMap::new())
            .expect("reconcile");
        assert_eq!(
            store.list_environments().expect("environments")[0].status,
            "stopped"
        );
    }
}
