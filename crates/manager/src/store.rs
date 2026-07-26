use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::{DateTime, Utc};
use domain::{
    AiAgentExecution, EnvironmentCacheStatus, EnvironmentRecord, FingerprintProfile, HostEvent,
    KernelRecord, ManagerEvent, ManagerSettings, OperationRecord, ProxyProfile,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 5;

pub struct StoredAgentExecution {
    pub plan_hash: String,
    pub state: String,
    pub execution: AiAgentExecution,
}

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
            data_dir: data_dir.display().to_string(),
            work_dir: platform::default_sdk_work_dir().display().to_string(),
            extension_dir: platform::default_extension_dir().display().to_string(),
            log_dir: platform::default_log_dir().display().to_string(),
            sdk_api_url: None,
            debug: false,
            startup_policy: "restore-none".into(),
            embedded_mcp_port: None,
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
                data_dir TEXT NOT NULL DEFAULT '',
                work_dir TEXT NOT NULL,
                extension_dir TEXT NOT NULL,
                log_dir TEXT NOT NULL,
                sdk_api_url TEXT,
                debug INTEGER NOT NULL,
                startup_policy TEXT NOT NULL DEFAULT 'restore-none',
                embedded_mcp_port INTEGER,
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

            CREATE TABLE IF NOT EXISTS environment_cache_status (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                state TEXT NOT NULL DEFAULT 'empty',
                cache_count INTEGER NOT NULL DEFAULT 0,
                last_success_at TEXT,
                last_attempt_at TEXT,
                last_error TEXT
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
                request_json TEXT,
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
                bound_env_ids_json TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS fingerprint_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'local',
                profile_json TEXT NOT NULL,
                bound_env_ids_json TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS environment_details (
                env_id TEXT PRIMARY KEY,
                detail_json TEXT NOT NULL,
                refreshed_at TEXT NOT NULL,
                FOREIGN KEY(env_id) REFERENCES environments(env_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS kernel_records (
                id TEXT PRIMARY KEY,
                kernel_type TEXT NOT NULL,
                name TEXT NOT NULL,
                major INTEGER,
                version TEXT,
                latest_version TEXT,
                platform TEXT NOT NULL,
                arch TEXT NOT NULL,
                status TEXT NOT NULL,
                install_path TEXT,
                download_available INTEGER NOT NULL DEFAULT 0,
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

            CREATE TABLE IF NOT EXISTS ai_agent_executions (
                idempotency_key TEXT PRIMARY KEY,
                plan_hash TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'completed',
                execution_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )?;
        ensure_column(
            &connection,
            "settings",
            "data_dir",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "settings",
            "startup_policy",
            "TEXT NOT NULL DEFAULT 'restore-none'",
        )?;
        ensure_column(&connection, "settings", "embedded_mcp_port", "INTEGER")?;
        ensure_column(&connection, "operations", "request_json", "TEXT")?;
        ensure_column(
            &connection,
            "proxy_profiles",
            "bound_env_ids_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        ensure_column(
            &connection,
            "fingerprint_profiles",
            "source",
            "TEXT NOT NULL DEFAULT 'local'",
        )?;
        ensure_column(
            &connection,
            "fingerprint_profiles",
            "bound_env_ids_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        ensure_column(
            &connection,
            "ai_agent_executions",
            "state",
            "TEXT NOT NULL DEFAULT 'completed'",
        )?;
        let now = timestamp();
        connection.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
            params![SCHEMA_VERSION, now],
        )?;
        connection.execute(
            r#"INSERT OR IGNORE INTO settings(
                id, data_dir, work_dir, extension_dir, log_dir, sdk_api_url, debug,
                startup_policy, embedded_mcp_port, updated_at
            ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                defaults.data_dir,
                defaults.work_dir,
                defaults.extension_dir,
                defaults.log_dir,
                defaults.sdk_api_url,
                defaults.debug,
                defaults.startup_policy,
                defaults.embedded_mcp_port,
                now,
            ],
        )?;
        connection.execute(
            "UPDATE settings SET data_dir = ?1 WHERE id = 1 AND data_dir = ''",
            [defaults.data_dir.as_str()],
        )?;
        connection.execute(
            "UPDATE environments SET local_label = '', tags_json = '[]'",
            [],
        )?;
        connection.execute(
            r#"INSERT OR IGNORE INTO environment_cache_status(id, state, cache_count)
               SELECT 1, CASE WHEN COUNT(*) > 0 THEN 'stale' ELSE 'empty' END, COUNT(*)
               FROM environments"#,
            [],
        )?;
        connection.execute(
            r#"UPDATE environment_cache_status SET
                   state = CASE WHEN (SELECT COUNT(*) FROM environments) > 0
                                THEN 'stale' ELSE 'empty' END,
                   cache_count = (SELECT COUNT(*) FROM environments)
               WHERE id = 1"#,
            [],
        )?;
        Ok(Self {
            path: Arc::new(path),
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn backup_to(&self, path: &Path) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
            })?;
        }
        let source = self.connection()?;
        let mut destination = Connection::open(path)?;
        let backup = rusqlite::backup::Backup::new(&source, &mut destination)?;
        backup.run_to_completion(32, std::time::Duration::from_millis(20), None)?;
        Ok(())
    }

    pub fn settings(&self) -> Result<ManagerSettings, StoreError> {
        self.connection()?
            .query_row(
                r#"SELECT data_dir, work_dir, extension_dir, log_dir, sdk_api_url, debug,
                      startup_policy, embedded_mcp_port
               FROM settings WHERE id = 1"#,
                [],
                |row| {
                    Ok(ManagerSettings {
                        data_dir: row.get(0)?,
                        work_dir: row.get(1)?,
                        extension_dir: row.get(2)?,
                        log_dir: row.get(3)?,
                        sdk_api_url: row.get(4)?,
                        debug: row.get(5)?,
                        startup_policy: row.get(6)?,
                        embedded_mcp_port: row.get(7)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn update_settings(&self, settings: &ManagerSettings) -> Result<(), StoreError> {
        self.connection()?.execute(
            r#"UPDATE settings SET
                data_dir = ?1, work_dir = ?2, extension_dir = ?3, log_dir = ?4,
                sdk_api_url = ?5, debug = ?6, startup_policy = ?7,
                embedded_mcp_port = ?8, updated_at = ?9
            WHERE id = 1"#,
            params![
                settings.data_dir,
                settings.work_dir,
                settings.extension_dir,
                settings.log_dir,
                settings.sdk_api_url,
                settings.debug,
                settings.startup_policy,
                settings.embedded_mcp_port,
                timestamp(),
            ],
        )?;
        Ok(())
    }

    pub fn list_environments(&self) -> Result<Vec<EnvironmentRecord>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT env_id, name, status, cdp, last_event,
                      generation, request_id, current_operation_id, updated_at
               FROM environments ORDER BY name, env_id"#,
        )?;
        let rows = statement.query_map([], environment_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn environment(&self, env_id: &str) -> Result<Option<EnvironmentRecord>, StoreError> {
        self.connection()?
            .query_row(
                r#"SELECT env_id, name, status, cdp, last_event,
                          generation, request_id, current_operation_id, updated_at
                   FROM environments WHERE env_id = ?1"#,
                [env_id],
                environment_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_remote_environments(
        &self,
        environments: &[(String, String, Value)],
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let now = timestamp();
        for (env_id, name, remote) in environments {
            let mut remote = remote.clone();
            sdk_ffi::redact_value(&mut remote);
            transaction.execute(
                r#"INSERT INTO environments(
                    env_id, name, local_label, tags_json, remote_json,
                    status, cdp, last_event, updated_at
                ) VALUES (?1, ?2, '', '[]', ?3, 'stopped', '-', 'env_page sync', ?4)
                ON CONFLICT(env_id) DO UPDATE SET
                    name = excluded.name,
                    local_label = '',
                    tags_json = '[]',
                    remote_json = excluded.remote_json,
                    updated_at = excluded.updated_at"#,
                params![env_id, name, serde_json::to_string(&remote)?, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn replace_remote_environments(
        &self,
        environments: &[(String, String, Value)],
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            r#"CREATE TEMP TABLE IF NOT EXISTS incoming_environment_ids (
                   env_id TEXT PRIMARY KEY
               );
               DELETE FROM incoming_environment_ids;"#,
        )?;
        let now = timestamp();
        for (env_id, name, remote) in environments {
            let mut remote = remote.clone();
            sdk_ffi::redact_value(&mut remote);
            transaction.execute(
                "INSERT OR IGNORE INTO incoming_environment_ids(env_id) VALUES (?1)",
                [env_id],
            )?;
            transaction.execute(
                r#"INSERT INTO environments(
                    env_id, name, local_label, tags_json, remote_json,
                    status, cdp, last_event, updated_at
                ) VALUES (?1, ?2, '', '[]', ?3, 'stopped', '-', 'env_page sync', ?4)
                ON CONFLICT(env_id) DO UPDATE SET
                    name = excluded.name,
                    local_label = '',
                    tags_json = '[]',
                    remote_json = excluded.remote_json,
                    updated_at = excluded.updated_at"#,
                params![env_id, name, serde_json::to_string(&remote)?, now],
            )?;
        }
        transaction.execute(
            "DELETE FROM runtime_snapshots WHERE env_id NOT IN (SELECT env_id FROM incoming_environment_ids)",
            [],
        )?;
        transaction.execute(
            "DELETE FROM environments WHERE env_id NOT IN (SELECT env_id FROM incoming_environment_ids)",
            [],
        )?;
        transaction.execute(
            r#"UPDATE environment_cache_status SET
                   state = 'fresh', cache_count = ?1, last_success_at = ?2,
                   last_attempt_at = ?2, last_error = NULL
               WHERE id = 1"#,
            params![environments.len() as i64, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn mark_environment_cache_stale(&self, error: &str) -> Result<(), StoreError> {
        self.connection()?.execute(
            r#"UPDATE environment_cache_status SET
                   state = CASE WHEN (SELECT COUNT(*) FROM environments) > 0
                                THEN 'stale' ELSE 'empty' END,
                   cache_count = (SELECT COUNT(*) FROM environments),
                   last_attempt_at = ?1,
                   last_error = ?2
               WHERE id = 1"#,
            params![timestamp(), error],
        )?;
        Ok(())
    }

    pub fn environment_cache_status(&self) -> Result<EnvironmentCacheStatus, StoreError> {
        let (state, count, last_success_at, last_attempt_at, last_error): (
            String,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = self.connection()?.query_row(
            r#"SELECT state, cache_count, last_success_at, last_attempt_at, last_error
               FROM environment_cache_status WHERE id = 1"#,
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        Ok(EnvironmentCacheStatus {
            source: "sdk-server".into(),
            state,
            count: count.max(0) as usize,
            last_success_at: last_success_at.as_deref().map(parse_time).transpose()?,
            last_attempt_at: last_attempt_at.as_deref().map(parse_time).transpose()?,
            last_error,
        })
    }

    pub fn delete_environment(&self, env_id: &str) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM runtime_snapshots WHERE env_id = ?1", [env_id])?;
        transaction.execute("DELETE FROM environments WHERE env_id = ?1", [env_id])?;
        transaction.execute(
            r#"UPDATE environment_cache_status SET
                   cache_count = (SELECT COUNT(*) FROM environments)
               WHERE id = 1"#,
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reset_account_state(&self) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            r#"DELETE FROM environment_details;
               DELETE FROM runtime_snapshots;
               DELETE FROM environments;
               DELETE FROM operations;
               DELETE FROM manager_events;
               DELETE FROM ai_agent_executions;
               UPDATE proxy_profiles SET bound_env_ids_json = '[]';
               UPDATE fingerprint_profiles SET bound_env_ids_json = '[]';
               UPDATE environment_cache_status SET
                   state = 'empty', cache_count = 0, last_success_at = NULL,
                   last_attempt_at = NULL, last_error = NULL
               WHERE id = 1;"#,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn save_environment_detail(&self, env_id: &str, detail: &Value) -> Result<(), StoreError> {
        self.connection()?.execute(
            r#"INSERT INTO environment_details(env_id, detail_json, refreshed_at)
               VALUES (?1, ?2, ?3)
               ON CONFLICT(env_id) DO UPDATE SET
                   detail_json = excluded.detail_json,
                   refreshed_at = excluded.refreshed_at"#,
            params![env_id, serde_json::to_string(detail)?, timestamp()],
        )?;
        Ok(())
    }

    pub fn environment_details(&self) -> Result<Vec<(String, Value, DateTime<Utc>)>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT env_id, detail_json, refreshed_at FROM environment_details ORDER BY env_id",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .map(|row| {
                let (env_id, detail, refreshed_at) = row?;
                Ok((
                    env_id,
                    serde_json::from_str(&detail)?,
                    parse_time(&refreshed_at)?,
                ))
            })
            .collect()
    }

    pub fn list_fingerprint_profiles(&self) -> Result<Vec<FingerprintProfile>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT id, name, source, profile_json, bound_env_ids_json, updated_at
               FROM fingerprint_profiles ORDER BY name, id"#,
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .map(|row| {
                let (id, name, source, profile, bound_env_ids, updated_at) = row?;
                Ok(FingerprintProfile {
                    id,
                    name,
                    source,
                    profile: serde_json::from_str(&profile)?,
                    bound_env_ids: serde_json::from_str(&bound_env_ids)?,
                    updated_at: parse_time(&updated_at)?,
                })
            })
            .collect()
    }

    pub fn upsert_fingerprint_profile(
        &self,
        id: &str,
        name: &str,
        source: &str,
        profile: &Value,
        bound_env_ids: &[String],
    ) -> Result<FingerprintProfile, StoreError> {
        self.connection()?.execute(
            r#"INSERT INTO fingerprint_profiles(
                   id, name, source, profile_json, bound_env_ids_json, updated_at
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   source = excluded.source,
                   profile_json = excluded.profile_json,
                   bound_env_ids_json = excluded.bound_env_ids_json,
                   updated_at = excluded.updated_at"#,
            params![
                id,
                name,
                source,
                serde_json::to_string(profile)?,
                serde_json::to_string(bound_env_ids)?,
                timestamp(),
            ],
        )?;
        self.list_fingerprint_profiles()?
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn delete_fingerprint_profile(&self, id: &str) -> Result<(), StoreError> {
        self.connection()?
            .execute("DELETE FROM fingerprint_profiles WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_proxy_profiles(&self) -> Result<Vec<ProxyProfile>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT id, name, scheme, host, port, username, secret_ref,
                      bound_env_ids_json, updated_at
               FROM proxy_profiles ORDER BY name, id"#,
        )?;
        statement
            .query_map([], |row| {
                let bound_env_ids: String = row.get(7)?;
                let updated_at: String = row.get(8)?;
                Ok(ProxyProfile {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    scheme: row.get(2)?,
                    host: row.get(3)?,
                    port: row.get::<_, i64>(4)? as u16,
                    username: row.get(5)?,
                    password_present: row.get::<_, Option<String>>(6)?.is_some(),
                    bound_env_ids: serde_json::from_str(&bound_env_ids).unwrap_or_default(),
                    updated_at: parse_time(&updated_at).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            8,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_proxy_profile(
        &self,
        id: &str,
        name: &str,
        scheme: &str,
        host: &str,
        port: u16,
        username: Option<&str>,
        secret_ref: Option<&str>,
        bound_env_ids: &[String],
    ) -> Result<ProxyProfile, StoreError> {
        self.connection()?.execute(
            r#"INSERT INTO proxy_profiles(
                   id, name, scheme, host, port, username, secret_ref,
                   bound_env_ids_json, updated_at
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   scheme = excluded.scheme,
                   host = excluded.host,
                   port = excluded.port,
                   username = excluded.username,
                   secret_ref = excluded.secret_ref,
                   bound_env_ids_json = excluded.bound_env_ids_json,
                   updated_at = excluded.updated_at"#,
            params![
                id,
                name,
                scheme,
                host,
                port,
                username,
                secret_ref,
                serde_json::to_string(bound_env_ids)?,
                timestamp(),
            ],
        )?;
        self.list_proxy_profiles()?
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn proxy_secret_ref(&self, id: &str) -> Result<Option<String>, StoreError> {
        self.connection()?
            .query_row(
                "SELECT secret_ref FROM proxy_profiles WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map(|value| value.flatten())
            .map_err(Into::into)
    }

    pub fn delete_proxy_profile(&self, id: &str) -> Result<Option<String>, StoreError> {
        let reference = self.proxy_secret_ref(id)?;
        self.connection()?
            .execute("DELETE FROM proxy_profiles WHERE id = ?1", [id])?;
        Ok(reference)
    }

    pub fn replace_kernel_records(&self, records: &[KernelRecord]) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM kernel_records", [])?;
        for record in records {
            transaction.execute(
                r#"INSERT INTO kernel_records(
                       id, kernel_type, name, major, version, latest_version, platform,
                       arch, status, install_path, download_available, updated_at
                   ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"#,
                params![
                    record.id,
                    record.kernel_type,
                    record.name,
                    record.major,
                    record.version,
                    record.latest_version,
                    record.platform,
                    record.arch,
                    record.status,
                    record.install_path,
                    record.download_available,
                    record.updated_at.to_rfc3339(),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_kernel_records(&self) -> Result<Vec<KernelRecord>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT id, kernel_type, name, major, version, latest_version, platform,
                      arch, status, install_path, download_available, updated_at
               FROM kernel_records ORDER BY major DESC, kernel_type, id"#,
        )?;
        statement
            .query_map([], |row| {
                let updated_at: String = row.get(11)?;
                Ok(KernelRecord {
                    id: row.get(0)?,
                    kernel_type: row.get(1)?,
                    name: row.get(2)?,
                    major: row.get(3)?,
                    version: row.get(4)?,
                    latest_version: row.get(5)?,
                    platform: row.get(6)?,
                    arch: row.get(7)?,
                    status: row.get(8)?,
                    install_path: row.get(9)?,
                    download_available: row.get(10)?,
                    updated_at: parse_time(&updated_at).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            11,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn delete_kernel_record(&self, id: &str) -> Result<(), StoreError> {
        self.connection()?
            .execute("DELETE FROM kernel_records WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn create_operation(
        &self,
        kind: &str,
        env_id: Option<&str>,
        label: &str,
        generation: u64,
        request: Option<&Value>,
    ) -> Result<OperationRecord, StoreError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO operations(
                id, kind, env_id, label, status, message, generation, request_json,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, 'queued', 'queued', ?5, ?6, ?7, ?7)"#,
            params![
                id,
                kind,
                env_id,
                label,
                generation,
                request.map(serde_json::to_string).transpose()?,
                now.to_rfc3339(),
            ],
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
                          generation, error_code, request_json, created_at, updated_at
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
                      generation, error_code, request_json, created_at, updated_at
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

    pub fn attach_operation_environment(
        &self,
        operation_id: &str,
        env_id: &str,
    ) -> Result<OperationRecord, StoreError> {
        self.connection()?.execute(
            "UPDATE operations SET env_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![env_id, timestamp(), operation_id],
        )?;
        self.operation(operation_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn update_operation_progress(
        &self,
        id: &str,
        request_id: Option<i32>,
        message: &str,
    ) -> Result<OperationRecord, StoreError> {
        self.connection()?.execute(
            r#"UPDATE operations SET request_id = COALESCE(?1, request_id),
                      message = ?2, updated_at = ?3
               WHERE id = ?4 AND status = 'running'"#,
            params![request_id, message, timestamp(), id],
        )?;
        self.operation(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn accept_environment_operation(
        &self,
        id: &str,
        request_id: Option<i32>,
        status: &str,
        cdp: &str,
        last_event: &str,
    ) -> Result<OperationRecord, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let operation =
            operation_tx(&transaction, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let now = timestamp();
        transaction.execute(
            "UPDATE operations SET request_id = COALESCE(?1, request_id), updated_at = ?2 WHERE id = ?3",
            params![request_id, now, id],
        )?;
        if matches!(operation.status.as_str(), "queued" | "running")
            && let Some(env_id) = operation.env_id.as_deref()
        {
            transaction.execute(
                r#"UPDATE environments SET status = ?1, request_id = ?2,
                          current_operation_id = ?3, cdp = ?4, last_event = ?5,
                          updated_at = ?6
                   WHERE env_id = ?7 AND generation = ?8"#,
                params![
                    status,
                    request_id,
                    id,
                    cdp,
                    last_event,
                    now,
                    env_id,
                    operation.generation,
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
                    env_id,
                    operation.generation,
                    request_id,
                    status,
                    cdp,
                    last_event,
                    now,
                ],
            )?;
        }
        transaction.commit()?;
        drop(connection);
        self.operation(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
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

    pub fn agent_execution(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<StoredAgentExecution>, StoreError> {
        self.connection()?
            .query_row(
                r#"SELECT plan_hash, state, execution_json
                   FROM ai_agent_executions WHERE idempotency_key = ?1"#,
                [idempotency_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(plan_hash, state, execution)| {
                Ok(StoredAgentExecution {
                    plan_hash,
                    state,
                    execution: serde_json::from_str(&execution)?,
                })
            })
            .transpose()
    }

    pub fn reserve_agent_execution(
        &self,
        idempotency_key: &str,
        plan_hash: &str,
        execution: &AiAgentExecution,
    ) -> Result<bool, StoreError> {
        let inserted = self.connection()?.execute(
            r#"INSERT INTO ai_agent_executions(
                   idempotency_key, plan_hash, state, execution_json, created_at
               ) VALUES (?1, ?2, 'running', ?3, ?4)
               ON CONFLICT(idempotency_key) DO NOTHING"#,
            params![
                idempotency_key,
                plan_hash,
                serde_json::to_string(execution)?,
                timestamp(),
            ],
        )?;
        Ok(inserted == 1)
    }

    pub fn complete_agent_execution(
        &self,
        idempotency_key: &str,
        execution: &AiAgentExecution,
    ) -> Result<(), StoreError> {
        self.connection()?.execute(
            r#"UPDATE ai_agent_executions
               SET state = 'completed', execution_json = ?1
               WHERE idempotency_key = ?2"#,
            params![serde_json::to_string(execution)?, idempotency_key],
        )?;
        Ok(())
    }

    pub fn mark_agent_execution_uncertain(&self, idempotency_key: &str) -> Result<(), StoreError> {
        self.connection()?.execute(
            "UPDATE ai_agent_executions SET state = 'uncertain' WHERE idempotency_key = ?1",
            [idempotency_key],
        )?;
        Ok(())
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
        {
            if let Some(request_id) = event.request_id {
                transaction.execute(
                    "UPDATE operations SET request_id = COALESCE(request_id, ?1), updated_at = ?2 WHERE id = ?3",
                    params![request_id, timestamp(), operation_id],
                )?;
                transaction.execute(
                    "UPDATE environments SET request_id = COALESCE(request_id, ?1), updated_at = ?2 WHERE current_operation_id = ?3",
                    params![request_id, timestamp(), operation_id],
                )?;
            }
            if !matches!(operation.status.as_str(), "queued" | "running") {
                transaction.commit()?;
                return Ok(manager_event);
            }
            if let Some(env_id) = event.env_id.as_deref().or(operation.env_id.as_deref()) {
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
            apply_async_operation_event(&transaction, &operation, event)?;
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
        observed: &std::collections::HashSet<String>,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let mut statement = transaction.prepare(
            r#"SELECT e.env_id, e.status, o.kind, o.status
               FROM environments e
               LEFT JOIN operations o ON o.id = e.current_operation_id"#,
        )?;
        let candidates = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for (env_id, previous, operation_kind, operation_status) in candidates {
            let active_start = operation_kind.as_deref() == Some("environment.start")
                && matches!(operation_status.as_deref(), Some("queued" | "running"));
            let (status, cdp, event) = match running.get(&env_id) {
                Some(cdp) => (
                    "ready",
                    cdp.as_str(),
                    "sdk_browser_info reconciliation: running",
                ),
                None if active_start => continue,
                None if observed.contains(&env_id) && previous == "stopped" => (
                    "unknown",
                    "-",
                    "sdk_browser_info reconciliation: active, readiness unknown",
                ),
                None if observed.contains(&env_id) => continue,
                None if matches!(
                    previous.as_str(),
                    "preparing" | "starting" | "ready" | "stopping" | "unknown"
                ) =>
                {
                    (
                        "stopped",
                        "-",
                        "sdk_browser_info reconciliation: not running",
                    )
                }
                None => continue,
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

fn apply_async_operation_event(
    transaction: &Transaction<'_>,
    operation: &OperationRecord,
    event: &HostEvent,
) -> Result<(), StoreError> {
    if operation.kind != "kernel.install" {
        return Ok(());
    }
    let event_name = event.event_name.to_ascii_lowercase();
    let status = if event_name.contains("install-success") {
        "succeeded"
    } else if event_name.contains("install-fail") || event_name.contains("error") || event.code < 0
    {
        "failed"
    } else {
        return Ok(());
    };
    transaction.execute(
        r#"UPDATE operations SET status = ?1, message = ?2,
                  request_id = COALESCE(request_id, ?3),
                  error_code = CASE WHEN ?1 = 'failed' THEN 'SDK_EVENT_FAILED' ELSE NULL END,
                  updated_at = ?4 WHERE id = ?5 AND status = 'running'"#,
        params![
            status,
            event.event_name,
            event.request_id,
            timestamp(),
            operation.id,
        ],
    )?;
    Ok(())
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
        r#"UPDATE operations SET status = ?1, message = ?2, request_id = COALESCE(request_id, ?3),
                  error_code = CASE WHEN ?1 = 'failed' THEN 'SDK_EVENT_FAILED' ELSE NULL END,
                  updated_at = ?4 WHERE id = ?5"#,
        params![
            operation_status,
            event.event_name,
            event.request_id,
            now,
            operation.id,
        ],
    )?;
    let cdp = if environment_status == "ready" {
        event_cdp(event)
    } else {
        "-".into()
    };
    transaction.execute(
        r#"UPDATE environments SET status = ?1, cdp = ?2, last_event = ?3,
                  request_id = COALESCE(request_id, ?4), current_operation_id = NULL, updated_at = ?5
           WHERE env_id = ?6 AND generation = ?7"#,
        params![
            environment_status,
            cdp,
            event.event_name,
            event.request_id,
            now,
            env_id,
            operation.generation,
        ],
    )?;
    Ok(())
}

fn event_cdp(event: &HostEvent) -> String {
    find_json_value(
        &event.payload,
        &["cdp", "cdpUrl", "debuggerAddress", "webSocketDebuggerUrl"],
    )
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .map(str::to_string)
    .or_else(|| {
        find_json_value(&event.payload, &["remoteDebuggingPort"])
            .and_then(Value::as_u64)
            .filter(|port| *port > 0)
            .map(|port| format!("127.0.0.1:{port}"))
    })
    .unwrap_or_else(|| "ready".into())
}

fn find_json_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key) {
                    return Some(value);
                }
            }
            map.values().find_map(|value| find_json_value(value, keys))
        }
        Value::Array(values) => values.iter().find_map(|value| find_json_value(value, keys)),
        _ => None,
    }
}

fn operation_tx(
    transaction: &Transaction<'_>,
    id: &str,
) -> Result<Option<OperationRecord>, StoreError> {
    transaction
        .query_row(
            r#"SELECT id, kind, env_id, label, status, message, request_id,
                      generation, error_code, request_json, created_at, updated_at
               FROM operations WHERE id = ?1"#,
            [id],
            operation_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn environment_from_row(row: &Row<'_>) -> rusqlite::Result<EnvironmentRecord> {
    let updated_at: String = row.get(8)?;
    Ok(EnvironmentRecord {
        env_id: row.get(0)?,
        name: row.get(1)?,
        status: row.get(2)?,
        cdp: row.get(3)?,
        last_event: row.get(4)?,
        generation: row.get::<_, i64>(5)? as u64,
        request_id: row.get(6)?,
        current_operation_id: row.get(7)?,
        updated_at: parse_time(&updated_at).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

fn operation_from_row(row: &Row<'_>) -> rusqlite::Result<OperationRecord> {
    let request: Option<String> = row.get(9)?;
    let created_at: String = row.get(10)?;
    let updated_at: String = row.get(11)?;
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
        request: request.and_then(|value| serde_json::from_str(&value).ok()),
        created_at: parse_time(&created_at).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        updated_at: parse_time(&updated_at).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                11,
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

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == column);
    if !exists {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))?;
    }
    Ok(())
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn test_store(directory: &tempfile::TempDir) -> ManagerStore {
        ManagerStore::open(
            directory.path().join("manager.sqlite3"),
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: "work".into(),
                extension_dir: "extensions".into(),
                log_dir: "logs".into(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
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
            .create_operation("environment.sync", None, "同步环境", 0, None)
            .expect("create operation");
        store
            .transition_operation(&operation.id, "running", "running", None)
            .expect("start operation");
        drop(store);

        let reopened = ManagerStore::open(
            path,
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: "unused".into(),
                extension_dir: "unused".into(),
                log_dir: "unused".into(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
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
        assert_eq!(
            reopened.settings().expect("settings").startup_policy,
            "restore-none"
        );
    }

    #[test]
    fn profiles_and_operation_requests_round_trip() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        let fingerprint = store
            .upsert_fingerprint_profile(
                "fp-1",
                "Desktop",
                "local",
                &json!({ "platform": "Win32" }),
                &["env-1".into()],
            )
            .expect("fingerprint");
        assert_eq!(fingerprint.bound_env_ids, vec!["env-1"]);

        let proxy = store
            .upsert_proxy_profile(
                "proxy-1",
                "Local",
                "socks5",
                "127.0.0.1",
                1080,
                Some("alice"),
                Some("proxy-1.bin"),
                &["env-1".into()],
            )
            .expect("proxy");
        assert!(proxy.password_present);
        assert_eq!(
            store
                .proxy_secret_ref("proxy-1")
                .expect("secret ref")
                .as_deref(),
            Some("proxy-1.bin")
        );

        let request = json!({ "cores": [{ "major": 141, "type": "yun" }] });
        let operation = store
            .create_operation("kernel.install", None, "安装内核", 0, Some(&request))
            .expect("operation");
        assert_eq!(operation.request, Some(request));
    }

    #[test]
    fn created_environment_can_be_attached_to_operation_and_deleted() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        let operation = store
            .create_operation("environment.create", None, "创建环境", 0, None)
            .expect("operation");
        store
            .upsert_remote_environments(&[(
                "env-created".into(),
                "Created".into(),
                json!({ "envId": "env-created" }),
            )])
            .expect("environment");
        let attached = store
            .attach_operation_environment(&operation.id, "env-created")
            .expect("attach environment");
        assert_eq!(attached.env_id.as_deref(), Some("env-created"));

        store
            .save_environment_detail("env-created", &json!({ "kernel": "Chrome" }))
            .expect("detail");
        store
            .delete_environment("env-created")
            .expect("delete environment");
        assert!(store.environment("env-created").expect("lookup").is_none());
        assert!(store.environment_details().expect("details").is_empty());
    }

    #[test]
    fn remote_environment_replace_is_complete_redacted_and_authoritative() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        store
            .upsert_remote_environments(&[
                (
                    "env-old".into(),
                    "Old".into(),
                    json!({ "envId": "env-old" }),
                ),
                (
                    "env-keep".into(),
                    "Before".into(),
                    json!({ "envId": "env-keep" }),
                ),
            ])
            .expect("seed environments");
        store
            .save_environment_detail("env-old", &json!({ "kernel": "Chrome" }))
            .expect("detail");
        store
            .connection()
            .expect("connection")
            .execute(
                "UPDATE environments SET local_label = 'Local', tags_json = '[\"tag\"]'",
                [],
            )
            .expect("legacy overrides");

        store
            .replace_remote_environments(&[
                (
                    "env-keep".into(),
                    "After".into(),
                    json!({
                        "envId": "env-keep",
                        "cookie": "private",
                        "proxy": "socks5://alice:secret@127.0.0.1:1080"
                    }),
                ),
                (
                    "env-new".into(),
                    "New".into(),
                    json!({ "envId": "env-new" }),
                ),
            ])
            .expect("replace environments");

        let environments = store.list_environments().expect("environments");
        assert_eq!(environments.len(), 2);
        assert_eq!(environments[0].name, "After");
        assert!(store.environment("env-old").expect("lookup").is_none());
        assert!(store.environment_details().expect("details").is_empty());
        let (label, tags, remote): (String, String, String) = store
            .connection()
            .expect("connection")
            .query_row(
                "SELECT local_label, tags_json, remote_json FROM environments WHERE env_id = 'env-keep'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("cached environment");
        assert!(label.is_empty());
        assert_eq!(tags, "[]");
        assert!(!remote.contains("private"));
        assert!(!remote.contains(":secret@"));
        let status = store.environment_cache_status().expect("cache status");
        assert_eq!(status.state, "fresh");
        assert_eq!(status.count, 2);
        assert!(status.last_success_at.is_some());
        assert!(status.last_error.is_none());
    }

    #[test]
    fn failed_refresh_preserves_cache_and_marks_it_stale() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        store
            .replace_remote_environments(&[(
                "env-1".into(),
                "Environment".into(),
                json!({ "envId": "env-1" }),
            )])
            .expect("fresh cache");

        store
            .mark_environment_cache_stale("second page failed")
            .expect("mark stale");

        assert!(store.environment("env-1").expect("lookup").is_some());
        let status = store.environment_cache_status().expect("cache status");
        assert_eq!(status.state, "stale");
        assert_eq!(status.count, 1);
        assert_eq!(status.last_error.as_deref(), Some("second page failed"));
        assert!(status.last_success_at.is_some());
        assert!(status.last_attempt_at.is_some());
    }

    #[test]
    fn account_reset_removes_remote_and_runtime_state_but_keeps_local_resources() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        store
            .upsert_remote_environments(&[(
                "env-1".into(),
                "Environment".into(),
                json!({ "envId": "env-1" }),
            )])
            .expect("environment");
        store
            .save_environment_detail("env-1", &json!({ "finger": { "language": "zh-CN" } }))
            .expect("detail");
        store
            .upsert_proxy_profile(
                "proxy-1",
                "Proxy",
                "socks5",
                "127.0.0.1",
                1080,
                None,
                None,
                &["env-1".into()],
            )
            .expect("proxy");
        store
            .create_operation("environment.start", Some("env-1"), "start", 1, None)
            .expect("operation");

        store.reset_account_state().expect("reset account state");

        assert!(store.list_environments().expect("environments").is_empty());
        assert!(store.environment_details().expect("details").is_empty());
        assert!(store.list_operations(10).expect("operations").is_empty());
        let proxies = store.list_proxy_profiles().expect("proxies");
        assert_eq!(proxies.len(), 1);
        assert!(proxies[0].bound_env_ids.is_empty());
        let status = store.environment_cache_status().expect("cache status");
        assert_eq!(status.state, "empty");
        assert_eq!(status.count, 0);
    }

    #[test]
    fn reopening_marks_cache_stale_and_removes_legacy_overrides() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("manager.sqlite3");
        let store = test_store(&directory);
        store
            .replace_remote_environments(&[(
                "env-1".into(),
                "Environment".into(),
                json!({ "envId": "env-1" }),
            )])
            .expect("fresh cache");
        store
            .connection()
            .expect("connection")
            .execute(
                "UPDATE environments SET local_label = 'Legacy', tags_json = '[\"old\"]'",
                [],
            )
            .expect("legacy overrides");
        drop(store);

        let reopened = ManagerStore::open(
            path,
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: "unused".into(),
                extension_dir: "unused".into(),
                log_dir: "unused".into(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
            },
        )
        .expect("reopen store");
        assert_eq!(
            reopened.environment_cache_status().expect("cache").state,
            "stale"
        );
        let (label, tags): (String, String) = reopened
            .connection()
            .expect("connection")
            .query_row(
                "SELECT local_label, tags_json FROM environments WHERE env_id = 'env-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("legacy columns");
        assert!(label.is_empty());
        assert_eq!(tags, "[]");
    }

    #[test]
    fn agent_execution_survives_reopen() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("manager.sqlite3");
        let store = test_store(&directory);
        let execution = AiAgentExecution {
            action: "none".into(),
            operation: None,
            response: Some(json!({ "summary": "read only" })),
            status_semantics: "No write action was executed.".into(),
            replayed: false,
        };
        assert!(
            store
                .reserve_agent_execution("key-1", "hash-1", &execution)
                .expect("reserve execution")
        );
        store
            .complete_agent_execution("key-1", &execution)
            .expect("complete execution");
        drop(store);

        let reopened = ManagerStore::open(
            path,
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: "unused".into(),
                extension_dir: "unused".into(),
                log_dir: "unused".into(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
            },
        )
        .expect("reopen store");
        let stored = reopened
            .agent_execution("key-1")
            .expect("execution")
            .expect("stored execution");
        assert_eq!(stored.plan_hash, "hash-1");
        assert_eq!(stored.state, "completed");
        assert_eq!(stored.execution.action, "none");
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
            .create_operation(
                "environment.start",
                Some("env-1"),
                "启动环境",
                generation,
                None,
            )
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
            .reconcile_running_environments(&HashMap::new(), &HashSet::new())
            .expect("reconcile");
        assert_eq!(
            store.list_environments().expect("environments")[0].status,
            "stopped"
        );
    }

    #[test]
    fn reconciliation_detects_external_start() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = test_store(&directory);
        store
            .upsert_remote_environments(&[(
                "env-1".into(),
                "Environment".into(),
                json!({ "envId": "env-1" }),
            )])
            .expect("upsert environment");
        store
            .reconcile_running_environments(
                &HashMap::from([("env-1".into(), "ws://127.0.0.1/devtools/browser/1".into())]),
                &HashSet::from(["env-1".into()]),
            )
            .expect("reconcile");
        let environment = store.list_environments().expect("environments").remove(0);
        assert_eq!(environment.status, "ready");
        assert_eq!(environment.cdp, "ws://127.0.0.1/devtools/browser/1");
    }

    #[test]
    fn accepted_response_does_not_rollback_early_success_event() {
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
        let operation = store
            .create_operation(
                "environment.start",
                Some("env-1"),
                "启动环境",
                generation,
                None,
            )
            .expect("operation");
        store
            .transition_operation(&operation.id, "running", "calling SDK", None)
            .expect("running");
        store
            .apply_host_event(&HostEvent {
                sequence: 1,
                event_type: "sdk.result".into(),
                code: 0,
                event_name: "browser-open-success".into(),
                request_id: Some(42),
                operation_id: Some(operation.id.clone()),
                env_id: Some("env-1".into()),
                payload: json!({}),
                received_at: Utc::now(),
            })
            .expect("early success");
        let operation = store
            .accept_environment_operation(
                &operation.id,
                None,
                "starting",
                "-",
                "SDK accepted request; awaiting callback reqId",
            )
            .expect("accepted response");
        let environment = store.list_environments().expect("environments").remove(0);
        assert_eq!(operation.status, "succeeded");
        assert_eq!(operation.request_id, Some(42));
        assert_eq!(environment.status, "ready");
    }
}
