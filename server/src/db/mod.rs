mod models;

pub use models::*;

use crate::error::{Error, Result};
use libsql::{Connection, Builder};
use std::sync::Arc;

const MIGRATION_SQL: &str = include_str!("../../migrations/001_init.sql");

#[derive(Clone)]
pub struct Db {
    conn: Connection,
}

impl Db {
    pub async fn new(url: &str, token: &str) -> Result<Self> {
        let conn = Builder::new_remote(url.to_string(), token.to_string())
            .build()
            .await?
            .connect()?;
        
        let db = Self { conn };
        db.run_migrations().await?;
        
        Ok(db)
    }
    
    async fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(MIGRATION_SQL).await?;
        Ok(())
    }
    
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
    
    pub async fn execute(&self, sql: &str, params: Vec<libsql::Value>) -> Result<u64> {
        let mut stmt = self.conn.prepare(sql).await?;
        let result = stmt.execute(&params).await?;
        Ok(result)
    }
    
    pub async fn query_row<T, F>(&self, sql: &str, params: Vec<libsql::Value>, f: F) -> Result<Option<T>>
    where
        F: Fn(&libsql::Row) -> Result<T>,
    {
        let mut stmt = self.conn.prepare(sql).await?;
        let mut rows = stmt.query(&params).await?;
        
        if let Some(row) = rows.next().await? {
            return Ok(Some(f(&row)?));
        }
        
        Ok(None)
    }
    
    pub async fn query_rows<T, F>(&self, sql: &str, params: Vec<libsql::Value>, f: F) -> Result<Vec<T>>
    where
        F: Fn(&libsql::Row) -> Result<T>,
    {
        let mut stmt = self.conn.prepare(sql).await?;
        let mut rows = stmt.query(&params).await?;
        let mut results = Vec::new();
        
        while let Some(row) = rows.next().await? {
            results.push(f(&row)?);
        }
        
        Ok(results)
    }
}

fn get_string(row: &libsql::Row, idx: usize) -> Result<String> {
    Ok(row.get::<String>(idx)?)
}

fn get_optional_string(row: &libsql::Row, idx: usize) -> Result<Option<String>> {
    let val: libsql::Value = row.get(idx)?;
    match val {
        libsql::Value::Null => Ok(None),
        libsql::Value::Text(s) => Ok(Some(s)),
        _ => Err(Error::Internal("Expected string or null".into())),
    }
}

fn get_i64(row: &libsql::Row, idx: usize) -> Result<i64> {
    Ok(row.get::<i64>(idx)?)
}

fn get_optional_i64(row: &libsql::Row, idx: usize) -> Result<Option<i64>> {
    let val: libsql::Value = row.get(idx)?;
    match val {
        libsql::Value::Null => Ok(None),
        libsql::Value::Integer(i) => Ok(Some(i)),
        _ => Err(Error::Internal("Expected integer or null".into())),
    }
}

fn get_bool(row: &libsql::Row, idx: usize) -> Result<bool> {
    Ok(row.get::<i64>(idx)? != 0)
}

fn get_u16(row: &libsql::Row, idx: usize) -> Result<u16> {
    Ok(row.get::<i64>(idx)? as u16)
}

fn get_optional_u16(row: &libsql::Row, idx: usize) -> Result<Option<u16>> {
    Ok(get_optional_i64(row, idx)?.map(|v| v as u16))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
