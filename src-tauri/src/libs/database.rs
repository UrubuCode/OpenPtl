use std::collections::HashMap;

use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};
use uuid::Uuid;

use crate::libs::models::{
    DatabaseDriver, DatabaseProfile, DbColumnDef, DbColumnMeta, DbForeignKey, DbIndex,
    DbQueryResult, DbTableDefinition, DbTableInfo,
};

pub enum DbPool {
    Postgres(sqlx::PgPool),
    MySQL(sqlx::MySqlPool),
    SQLite(sqlx::SqlitePool),
}

pub struct DbSessionManager {
    sessions: HashMap<String, DbPool>,
    active_databases: HashMap<String, String>,
}

impl DbSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            active_databases: HashMap::new(),
        }
    }

    pub async fn connect(&mut self, profile: &DatabaseProfile) -> Result<String> {
        let pool = build_pool(profile).await?;
        let session_id = Uuid::new_v4().to_string();
        if let Some(db) = &profile.database {
            if !db.is_empty() {
                self.active_databases.insert(session_id.clone(), db.clone());
            }
        }
        self.sessions.insert(session_id.clone(), pool);
        Ok(session_id)
    }

    pub fn disconnect(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
        self.active_databases.remove(session_id);
    }

    pub fn set_database(&mut self, session_id: &str, database: String) -> Result<()> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| anyhow!("Sessao de banco nao encontrada: {}", session_id))?;
        self.active_databases.insert(session_id.to_string(), database);
        Ok(())
    }

    pub fn get(&self, session_id: &str) -> Result<&DbPool> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| anyhow!("Sessao de banco nao encontrada: {}", session_id))
    }

    pub fn get_active_database(&self, session_id: &str) -> Option<&str> {
        self.active_databases.get(session_id).map(|s| s.as_str())
    }
}

async fn build_pool(profile: &DatabaseProfile) -> Result<DbPool> {
    let timeout = std::time::Duration::from_secs(10);
    match &profile.driver {
        DatabaseDriver::Sqlite => {
            let path = profile
                .file_path
                .as_deref()
                .ok_or_else(|| anyhow!("Caminho do arquivo SQLite nao informado"))?;
            let url = format!("sqlite:{}", path);
            let pool = tokio::time::timeout(timeout, sqlx::SqlitePool::connect(&url))
                .await
                .map_err(|_| anyhow!("Connection timed out"))??;
            Ok(DbPool::SQLite(pool))
        }
        DatabaseDriver::Postgres => {
            let url = build_url(profile, 5432, "postgres");
            let pool = tokio::time::timeout(timeout, sqlx::PgPool::connect(&url))
                .await
                .map_err(|_| anyhow!("Connection timed out"))??;
            Ok(DbPool::Postgres(pool))
        }
        DatabaseDriver::Mysql | DatabaseDriver::Mariadb => {
            let url = build_url(profile, 3306, "mysql");
            let pool = tokio::time::timeout(timeout, sqlx::MySqlPool::connect(&url))
                .await
                .map_err(|_| anyhow!("Connection timed out"))??;
            Ok(DbPool::MySQL(pool))
        }
    }
}

fn build_url(profile: &DatabaseProfile, default_port: u16, scheme: &str) -> String {
    let host = profile.host.as_deref().unwrap_or("127.0.0.1");
    let port = profile.port.unwrap_or(default_port);
    let user = urlencoding::encode(profile.username.as_deref().unwrap_or(""));
    let pass = profile
        .password
        .as_deref()
        .map(|p| format!(":{}@", urlencoding::encode(p)))
        .unwrap_or_else(|| "@".to_string());
    let db_path = match profile.database.as_deref() {
        Some(db) if !db.is_empty() => format!("/{}", db),
        _ => String::new(),
    };
    let ssl = if profile.ssl { "?sslmode=require" } else { "" };
    format!("{}://{}{}{}:{}{}{}", scheme, user, pass, host, port, db_path, ssl)
}

pub async fn db_query_exec(pool: &DbPool, sql: &str, active_db: Option<&str>) -> Result<DbQueryResult> {
    let start = std::time::Instant::now();
    match pool {
        DbPool::Postgres(p) => query_pg(p, sql, start).await,
        DbPool::MySQL(p) => query_mysql(p, sql, active_db, start).await,
        DbPool::SQLite(p) => query_sqlite(p, sql, start).await,
    }
}

async fn query_pg(
    pool: &sqlx::PgPool,
    sql: &str,
    start: std::time::Instant,
) -> Result<DbQueryResult> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    let duration_ms = start.elapsed().as_millis() as u64;

    let columns: Vec<DbColumnMeta> = if let Some(first) = rows.first() {
        first
            .columns()
            .iter()
            .map(|c| DbColumnMeta {
                name: c.name().to_string(),
                type_name: c.type_info().name().to_string(),
                nullable: true,
            })
            .collect()
    } else {
        vec![]
    };

    let result_rows: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| {
            (0..row.len())
                .map(|i| pg_value_to_json(row, i))
                .collect()
        })
        .collect();

    Ok(DbQueryResult {
        columns,
        rows: result_rows,
        affected_rows: 0,
        duration_ms,
    })
}

fn pg_value_to_json(row: &sqlx::postgres::PgRow, idx: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return v.map(Value::Bool).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v
            .map(|n| Value::Number(n.into()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
        return v
            .map(|n| Value::Number(n.into()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return v
            .map(|n| serde_json::json!(n))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return v.map(Value::String).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
        return v.unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return v
            .map(|b| Value::String(base64::engine::general_purpose::STANDARD.encode(b)))
            .unwrap_or(Value::Null);
    }
    Value::Null
}

async fn query_mysql(
    pool: &sqlx::MySqlPool,
    sql: &str,
    active_db: Option<&str>,
    start: std::time::Instant,
) -> Result<DbQueryResult> {
    let mut conn = pool.acquire().await?;
    if let Some(db) = active_db {
        sqlx::query(&format!("USE `{}`", db))
            .execute(&mut *conn)
            .await?;
    }
    let rows = match sqlx::query(sql).fetch_all(&mut *conn).await {
        Ok(rows) => rows,
        Err(e) => {
            let is_text_protocol_only = matches!(&e, sqlx::Error::Database(db_err)
                if db_err.message().contains("1295")
                    || db_err.message().contains("prepared statement protocol"));
            if is_text_protocol_only {
                sqlx::raw_sql(sql).fetch_all(pool).await?
            } else {
                return Err(e.into());
            }
        }
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    let columns: Vec<DbColumnMeta> = if let Some(first) = rows.first() {
        first
            .columns()
            .iter()
            .map(|c| DbColumnMeta {
                name: c.name().to_string(),
                type_name: c.type_info().name().to_string(),
                nullable: true,
            })
            .collect()
    } else {
        vec![]
    };

    let result_rows: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| (0..row.len()).map(|i| mysql_value_to_json(row, i)).collect())
        .collect();

    Ok(DbQueryResult {
        columns,
        rows: result_rows,
        affected_rows: 0,
        duration_ms,
    })
}

fn mysql_value_to_json(row: &sqlx::mysql::MySqlRow, idx: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return v.map(Value::Bool).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v.map(|n| Value::Number(n.into())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
        return v.map(|n| Value::Number(n.into())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return v.map(|n| serde_json::json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return v.map(Value::String).unwrap_or(Value::Null);
    }
    // Binary/VARBINARY columns (e.g. mysql.user.User/Host): try UTF-8 first so
    // text values don't get base64-encoded. Fall back to base64 only for true blobs.
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return v
            .map(|b| {
                String::from_utf8(b.clone())
                    .map(Value::String)
                    .unwrap_or_else(|_| {
                        Value::String(base64::engine::general_purpose::STANDARD.encode(b))
                    })
            })
            .unwrap_or(Value::Null);
    }
    Value::Null
}

async fn query_sqlite(
    pool: &sqlx::SqlitePool,
    sql: &str,
    start: std::time::Instant,
) -> Result<DbQueryResult> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    let duration_ms = start.elapsed().as_millis() as u64;

    let columns: Vec<DbColumnMeta> = if let Some(first) = rows.first() {
        first
            .columns()
            .iter()
            .map(|c| DbColumnMeta {
                name: c.name().to_string(),
                type_name: c.type_info().name().to_string(),
                nullable: true,
            })
            .collect()
    } else {
        vec![]
    };

    let result_rows: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| {
            (0..row.len())
                .map(|i| sqlite_value_to_json(row, i))
                .collect()
        })
        .collect();

    Ok(DbQueryResult {
        columns,
        rows: result_rows,
        affected_rows: 0,
        duration_ms,
    })
}

fn sqlite_value_to_json(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return v.map(Value::Bool).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v
            .map(|n| Value::Number(n.into()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return v.map(|n| serde_json::json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return v.map(Value::String).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return v
            .map(|b| Value::String(base64::engine::general_purpose::STANDARD.encode(b)))
            .unwrap_or(Value::Null);
    }
    Value::Null
}

pub async fn db_list_databases(pool: &DbPool) -> Result<Vec<String>> {
    match pool {
        DbPool::Postgres(p) => {
            let rows =
                sqlx::query("SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname")
                    .fetch_all(p)
                    .await?;
            Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
        }
        DbPool::MySQL(p) => {
            let rows = sqlx::query("SHOW DATABASES").fetch_all(p).await?;
            Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
        }
        DbPool::SQLite(_) => Ok(vec!["main".to_string()]),
    }
}

pub async fn db_list_schemas(pool: &DbPool, database: &str) -> Result<Vec<String>> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT schema_name FROM information_schema.schemata WHERE catalog_name = $1 ORDER BY schema_name",
            )
            .bind(database)
            .fetch_all(p)
            .await?;
            Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
        }
        DbPool::MySQL(_) => Ok(vec![database.to_string()]),
        DbPool::SQLite(_) => Ok(vec!["main".to_string()]),
    }
}

pub async fn db_list_tables(
    pool: &DbPool,
    database: &str,
    schema: &str,
) -> Result<Vec<DbTableInfo>> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT table_name, table_type FROM information_schema.tables WHERE table_catalog = $1 AND table_schema = $2 ORDER BY table_name",
            )
            .bind(database)
            .bind(schema)
            .fetch_all(p)
            .await?;
            Ok(rows
                .iter()
                .map(|r| DbTableInfo {
                    name: r.get::<String, _>(0),
                    schema: schema.to_string(),
                    kind: if r.get::<String, _>(1) == "VIEW" {
                        "view".to_string()
                    } else {
                        "table".to_string()
                    },
                    rows: None, size_bytes: None, engine: None, comment: None,
                    created_at: None, updated_at: None, collation: None,
                    auto_increment: None, data_free: None, index_length: None,
                    avg_row_length: None, max_data_length: None, check_time: None,
                    checksum: None, create_options: None, row_format: None, version: None,
                })
                .collect())
        }
        DbPool::MySQL(p) => {
            let rows = sqlx::query(
                "SELECT TABLE_NAME, TABLE_TYPE, \
                 COALESCE(TABLE_ROWS, 0), \
                 COALESCE(DATA_LENGTH + INDEX_LENGTH, 0), \
                 ENGINE, TABLE_COMMENT, \
                 DATE_FORMAT(CREATE_TIME, '%Y-%m-%d %H:%i:%s'), \
                 DATE_FORMAT(UPDATE_TIME, '%Y-%m-%d %H:%i:%s'), \
                 TABLE_COLLATION, AUTO_INCREMENT, DATA_FREE, INDEX_LENGTH, \
                 AVG_ROW_LENGTH, MAX_DATA_LENGTH, \
                 DATE_FORMAT(CHECK_TIME, '%Y-%m-%d %H:%i:%s'), \
                 CHECKSUM, CREATE_OPTIONS, ROW_FORMAT, VERSION \
                 FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
            )
            .bind(database)
            .fetch_all(p)
            .await?;
            Ok(rows
                .iter()
                .map(|r| DbTableInfo {
                    name: r.get::<String, _>(0),
                    schema: database.to_string(),
                    kind: if r.get::<String, _>(1).contains("VIEW") {
                        "view".to_string()
                    } else {
                        "table".to_string()
                    },
                    rows: r.try_get::<Option<i64>, _>(2).ok().flatten(),
                    size_bytes: r.try_get::<Option<i64>, _>(3).ok().flatten(),
                    engine: r.try_get::<Option<String>, _>(4).ok().flatten(),
                    comment: r.try_get::<Option<String>, _>(5).ok().flatten(),
                    created_at: r.try_get::<Option<String>, _>(6).ok().flatten(),
                    updated_at: r.try_get::<Option<String>, _>(7).ok().flatten(),
                    collation: r.try_get::<Option<String>, _>(8).ok().flatten(),
                    auto_increment: r.try_get::<Option<i64>, _>(9).ok().flatten(),
                    data_free: r.try_get::<Option<i64>, _>(10).ok().flatten(),
                    index_length: r.try_get::<Option<i64>, _>(11).ok().flatten(),
                    avg_row_length: r.try_get::<Option<i64>, _>(12).ok().flatten(),
                    max_data_length: r.try_get::<Option<i64>, _>(13).ok().flatten(),
                    check_time: r.try_get::<Option<String>, _>(14).ok().flatten(),
                    checksum: r.try_get::<Option<i64>, _>(15).ok().flatten(),
                    create_options: r.try_get::<Option<String>, _>(16).ok().flatten(),
                    row_format: r.try_get::<Option<String>, _>(17).ok().flatten(),
                    version: r.try_get::<Option<i64>, _>(18).ok().flatten(),
                })
                .collect())
        }
        DbPool::SQLite(p) => {
            let rows = sqlx::query(
                "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') ORDER BY name",
            )
            .fetch_all(p)
            .await?;
            Ok(rows
                .iter()
                .map(|r| DbTableInfo {
                    name: r.get::<String, _>(0),
                    schema: "main".to_string(),
                    kind: r.get::<String, _>(1),
                    rows: None, size_bytes: None, engine: None, comment: None,
                    created_at: None, updated_at: None, collation: None,
                    auto_increment: None, data_free: None, index_length: None,
                    avg_row_length: None, max_data_length: None, check_time: None,
                    checksum: None, create_options: None, row_format: None, version: None,
                })
                .collect())
        }
    }
}

pub async fn db_describe_table(
    pool: &DbPool,
    database: &str,
    schema: &str,
    table: &str,
) -> Result<DbTableDefinition> {
    match pool {
        DbPool::Postgres(p) => describe_pg(p, schema, table).await,
        DbPool::MySQL(p) => describe_mysql(p, database, table).await,
        DbPool::SQLite(p) => describe_sqlite(p, table).await,
    }
}

async fn describe_pg(
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
) -> Result<DbTableDefinition> {
    let col_rows = sqlx::query(
        "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default,
                CASE WHEN kcu.column_name IS NOT NULL THEN true ELSE false END as is_pk
         FROM information_schema.columns c
         LEFT JOIN information_schema.key_column_usage kcu
           ON kcu.table_schema = c.table_schema AND kcu.table_name = c.table_name AND kcu.column_name = c.column_name
           AND EXISTS (
             SELECT 1 FROM information_schema.table_constraints tc
             WHERE tc.constraint_name = kcu.constraint_name AND tc.constraint_type = 'PRIMARY KEY'
           )
         WHERE c.table_schema = $1 AND c.table_name = $2
         ORDER BY c.ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    let columns: Vec<DbColumnDef> = col_rows
        .iter()
        .map(|r| DbColumnDef {
            name: r.get::<String, _>(0),
            data_type: r.get::<String, _>(1),
            nullable: r.get::<String, _>(2) == "YES",
            default_value: r.try_get::<Option<String>, _>(3).ok().flatten(),
            is_primary_key: r.try_get::<bool, _>(4).unwrap_or(false),
            extra: None,
            comment: None,
        })
        .collect();

    let primary_key: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect();

    let idx_rows = sqlx::query(
        "SELECT i.relname, ix.indisunique, array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum))
         FROM pg_class t
         JOIN pg_index ix ON t.oid = ix.indrelid
         JOIN pg_class i ON i.oid = ix.indexrelid
         JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
         JOIN pg_namespace n ON n.oid = t.relnamespace
         WHERE t.relname = $1 AND n.nspname = $2
         GROUP BY i.relname, ix.indisunique",
    )
    .bind(table)
    .bind(schema)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let indexes: Vec<DbIndex> = idx_rows
        .iter()
        .map(|r| {
            let cols_str: String = r.try_get::<String, _>(2).unwrap_or_default();
            let cols: Vec<String> = cols_str
                .trim_matches(|c| c == '{' || c == '}')
                .split(',')
                .map(|s| s.to_string())
                .collect();
            DbIndex {
                name: r.get::<String, _>(0),
                unique: r.try_get::<bool, _>(1).unwrap_or(false),
                columns: cols,
            }
        })
        .collect();

    let fk_rows = sqlx::query(
        "SELECT tc.constraint_name, kcu.column_name, ccu.table_name, ccu.column_name
         FROM information_schema.table_constraints tc
         JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
         JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_name = tc.constraint_name
         WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut fk_map: HashMap<String, DbForeignKey> = HashMap::new();
    for r in &fk_rows {
        let name: String = r.get(0);
        let col: String = r.get(1);
        let ref_table: String = r.get(2);
        let ref_col: String = r.get(3);
        let entry = fk_map.entry(name.clone()).or_insert(DbForeignKey {
            name,
            columns: vec![],
            ref_table,
            ref_columns: vec![],
        });
        entry.columns.push(col);
        entry.ref_columns.push(ref_col);
    }

    Ok(DbTableDefinition {
        columns,
        indexes,
        foreign_keys: fk_map.into_values().collect(),
        primary_key,
        ddl: None,
    })
}

async fn describe_mysql(pool: &sqlx::MySqlPool, database: &str, table: &str) -> Result<DbTableDefinition> {
    let col_rows = sqlx::query(
        "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY, COLUMN_DEFAULT, EXTRA, COLUMN_COMMENT \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
         ORDER BY ORDINAL_POSITION"
    )
    .bind(database)
    .bind(table)
    .fetch_all(pool)
    .await?;

    let columns: Vec<DbColumnDef> = col_rows.iter().map(|r| DbColumnDef {
        name: r.get::<String, _>(0),
        data_type: r.get::<String, _>(1),
        nullable: r.try_get::<String, _>(2).unwrap_or_default() == "YES",
        is_primary_key: r.try_get::<String, _>(3).unwrap_or_default() == "PRI",
        default_value: r.try_get::<Option<String>, _>(4).ok().flatten(),
        extra: r.try_get::<Option<String>, _>(5).ok().flatten().filter(|s| !s.is_empty()),
        comment: r.try_get::<Option<String>, _>(6).ok().flatten().filter(|s| !s.is_empty()),
    }).collect();

    let primary_key = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect();

    // DDL via SHOW CREATE TABLE — use raw_sql (text protocol) because MariaDB
    // does not support this statement in the prepared-statement protocol (1295).
    let ddl: Option<String> = sqlx::raw_sql(&format!("SHOW CREATE TABLE `{}`.`{}`", database, table))
        .fetch_all(pool)
        .await
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .and_then(|r: sqlx::mysql::MySqlRow| r.try_get::<String, _>(1).ok());

    // Indexes from information_schema
    let idx_rows = sqlx::query(
        "SELECT INDEX_NAME, NON_UNIQUE, GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX SEPARATOR ',') \
         FROM information_schema.STATISTICS \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
         GROUP BY INDEX_NAME, NON_UNIQUE",
    )
    .bind(database)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let indexes: Vec<DbIndex> = idx_rows
        .iter()
        .map(|r| {
            let cols_str: String = r.try_get::<String, _>(2).unwrap_or_default();
            DbIndex {
                name: r.get::<String, _>(0),
                unique: r.try_get::<i8, _>(1).unwrap_or(1) == 0,
                columns: cols_str.split(',').map(|s| s.to_string()).collect(),
            }
        })
        .collect();

    // Foreign keys from information_schema
    let fk_rows = sqlx::query(
        "SELECT CONSTRAINT_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME \
         FROM information_schema.KEY_COLUMN_USAGE \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND REFERENCED_TABLE_NAME IS NOT NULL \
         ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION",
    )
    .bind(database)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut fk_map: std::collections::HashMap<String, DbForeignKey> =
        std::collections::HashMap::new();
    for r in &fk_rows {
        let name: String = r.get(0);
        let col: String = r.get(1);
        let ref_table: String = r.get(2);
        let ref_col: String = r.get(3);
        let fk = fk_map.entry(name.clone()).or_insert(DbForeignKey {
            name,
            columns: vec![],
            ref_table,
            ref_columns: vec![],
        });
        fk.columns.push(col);
        fk.ref_columns.push(ref_col);
    }

    Ok(DbTableDefinition {
        columns,
        indexes,
        foreign_keys: fk_map.into_values().collect(),
        primary_key,
        ddl,
    })
}

async fn describe_sqlite(pool: &sqlx::SqlitePool, table: &str) -> Result<DbTableDefinition> {
    let col_rows = sqlx::query(&format!("PRAGMA table_info(`{}`)", table))
        .fetch_all(pool)
        .await?;

    let columns: Vec<DbColumnDef> = col_rows
        .iter()
        .map(|r| DbColumnDef {
            name: r.get::<String, _>(1),
            data_type: r.get::<String, _>(2),
            nullable: r.try_get::<i32, _>(3).unwrap_or(0) == 0,
            default_value: r.try_get::<Option<String>, _>(4).ok().flatten(),
            is_primary_key: r.try_get::<i32, _>(5).unwrap_or(0) == 1,
            extra: None,
            comment: None,
        })
        .collect();

    let primary_key = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect();

    let idx_rows =
        sqlx::query(&format!("PRAGMA index_list(`{}`)", table))
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    let mut indexes = vec![];
    for r in &idx_rows {
        let idx_name: String = r.get(1);
        let unique: i32 = r.try_get(2).unwrap_or(0);
        let info_rows =
            sqlx::query(&format!("PRAGMA index_info(`{}`)", idx_name))
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        let cols: Vec<String> = info_rows.iter().map(|ir| ir.get::<String, _>(2)).collect();
        indexes.push(DbIndex {
            name: idx_name,
            unique: unique == 1,
            columns: cols,
        });
    }

    let ddl_row = sqlx::query(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name = ?",
    )
    .bind(table)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let ddl = ddl_row.and_then(|r| r.try_get::<String, _>(0).ok());

    Ok(DbTableDefinition {
        columns,
        indexes,
        foreign_keys: vec![],
        primary_key,
        ddl,
    })
}

pub async fn db_get_table_data(
    pool: &DbPool,
    database: &str,
    schema: &str,
    table: &str,
    limit: u32,
    offset: u32,
) -> Result<DbQueryResult> {
    let sql = match pool {
        DbPool::Postgres(_) => format!(
            "SELECT * FROM \"{}\".\"{}\" LIMIT {} OFFSET {}",
            schema, table, limit, offset
        ),
        DbPool::MySQL(_) => format!(
            "SELECT * FROM `{}`.`{}` LIMIT {} OFFSET {}",
            database, table, limit, offset
        ),
        DbPool::SQLite(_) => format!(
            "SELECT * FROM `{}` LIMIT {} OFFSET {}",
            table, limit, offset
        ),
    };
    db_query_exec(pool, &sql, None).await
}
