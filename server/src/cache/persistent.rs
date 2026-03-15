use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row, postgres::PgRow, QueryBuilder};

use crate::common::error::AppError;
use crate::common::types::{Ecosystem, ModuleFormat, SizeResult};

const SELECT_COLS: &str = "\
    name, version, size, gzip, brotli, total_size, total_gzip, total_brotli, \
    dependency_count, treeshakeable, side_effects, module_format, \
    keywords, description, dependency_names, \
    repository_url, homepage, license, unpacked_size, has_types, \
    monthly_downloads, node_engine, maintainers, cached_at";

fn row_to_result(row: &PgRow, ecosystem: &str) -> Result<SizeResult, AppError> {
    let eco: Ecosystem = ecosystem.parse().map_err(AppError::Internal)?;
    let module_format: ModuleFormat = row.get::<String, _>("module_format").parse().unwrap();
    let cached_at: DateTime<Utc> = row.get("cached_at");

    Ok(SizeResult {
        name: row.get("name"),
        version: row.get("version"),
        description: row.get("description"),
        keywords: row.get("keywords"),
        ecosystem: eco,
        size: row.get::<i64, _>("size") as u64,
        gzip: row.get::<i64, _>("gzip") as u64,
        brotli: row.get::<i64, _>("brotli") as u64,
        total_size: row.get::<i64, _>("total_size") as u64,
        total_gzip: row.get::<i64, _>("total_gzip") as u64,
        total_brotli: row.get::<i64, _>("total_brotli") as u64,
        dependency_count: row.get::<i32, _>("dependency_count") as u32,
        dependency_names: row.get("dependency_names"),
        treeshakeable: row.get("treeshakeable"),
        side_effects: row.get("side_effects"),
        module_format,
        repository_url: row.get("repository_url"),
        homepage: row.get("homepage"),
        license: row.get("license"),
        unpacked_size: row.get::<i64, _>("unpacked_size") as u64,
        has_types: row.get("has_types"),
        monthly_downloads: row.get::<i64, _>("monthly_downloads") as u64,
        node_engine: row.get("node_engine"),
        maintainers: row.get("maintainers"),
        cached_at,
    })
}

// Whitelisted sort columns — never interpolate user input directly.
fn sort_col(s: &str) -> &'static str {
    match s {
        "size" => "size",
        "brotli" => "brotli",
        "total_gzip" => "total_gzip",
        "total_size" => "total_size",
        "name" => "name",
        "downloads" | "monthly_downloads" => "monthly_downloads",
        _ => "gzip",
    }
}

#[derive(Clone)]
pub struct PersistentCache {
    pool: PgPool,
}

impl PersistentCache {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, ecosystem: &str, name: &str, version: &str) -> Result<Option<SizeResult>, AppError> {
        // npm is the only ecosystem with a dedicated table for now.
        // Future ecosystems add a match arm here.
        let table = ecosystem_table(ecosystem)?;
        let sql = format!("SELECT {SELECT_COLS} FROM {table} WHERE name = $1 AND version = $2");

        sqlx::query(&sql)
            .bind(name)
            .bind(version)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| row_to_result(&r, ecosystem))
            .transpose()
    }

    pub async fn get_latest(&self, ecosystem: &str, name: &str) -> Result<Option<SizeResult>, AppError> {
        let table = ecosystem_table(ecosystem)?;
        let sql = format!(
            "SELECT {SELECT_COLS} FROM {table} WHERE name = $1 ORDER BY cached_at DESC LIMIT 1"
        );

        sqlx::query(&sql)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| row_to_result(&r, ecosystem))
            .transpose()
    }

    pub async fn put(&self, ecosystem: &str, name: &str, version: &str, result: &SizeResult) -> Result<(), AppError> {
        let table = ecosystem_table(ecosystem)?;
        let mf = serde_json::to_value(&result.module_format)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "unknown".into());

        let sql = format!(
            r#"INSERT INTO {table} (
                name, version,
                size, gzip, brotli,
                total_size, total_gzip, total_brotli,
                dependency_count, treeshakeable, module_format,
                keywords, description, side_effects, dependency_names,
                repository_url, homepage, license, unpacked_size,
                has_types, monthly_downloads, node_engine, maintainers
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23
            ) ON CONFLICT (name, version) DO NOTHING"#
        );

        sqlx::query(&sql)
            .bind(name).bind(version)
            .bind(result.size as i64).bind(result.gzip as i64).bind(result.brotli as i64)
            .bind(result.total_size as i64).bind(result.total_gzip as i64).bind(result.total_brotli as i64)
            .bind(result.dependency_count as i32).bind(result.treeshakeable).bind(&mf)
            .bind(&result.keywords).bind(&result.description).bind(result.side_effects).bind(&result.dependency_names)
            .bind(&result.repository_url).bind(&result.homepage).bind(&result.license).bind(result.unpacked_size as i64)
            .bind(result.has_types).bind(result.monthly_downloads as i64).bind(&result.node_engine).bind(&result.maintainers)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn search(
        &self,
        ecosystem: &str,
        query: Option<&str>,
        keyword: Option<&str>,
        treeshakeable: Option<bool>,
        has_types: Option<bool>,
        sort: &str,
        order: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SizeResult>, AppError> {
        let table = ecosystem_table(ecosystem)?;
        let col = sort_col(sort);
        let dir = if order == "desc" { "DESC" } else { "ASC" };

        // DISTINCT ON to get the latest version per package, then apply filters and sort.
        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(format!(
            "SELECT {SELECT_COLS} FROM (\
                SELECT DISTINCT ON (name) * FROM {table} ORDER BY name, cached_at DESC\
            ) sub WHERE true"
        ));

        if let Some(q) = query {
            qb.push(" AND name % ");
            qb.push_bind(q);
        }
        if let Some(kw) = keyword {
            qb.push(" AND ");
            qb.push_bind(kw);
            qb.push(" = ANY(keywords)");
        }
        if let Some(ts) = treeshakeable {
            qb.push(" AND treeshakeable = ");
            qb.push_bind(ts);
        }
        if let Some(ht) = has_types {
            qb.push(" AND has_types = ");
            qb.push_bind(ht);
        }

        qb.push(format!(" ORDER BY {col} {dir} LIMIT "));
        qb.push_bind(limit);
        qb.push(" OFFSET ");
        qb.push_bind(offset);

        qb.build()
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| row_to_result(r, ecosystem))
            .collect()
    }

    pub async fn top(
        &self,
        ecosystem: &str,
        sort: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SizeResult>, AppError> {
        let table = ecosystem_table(ecosystem)?;
        let col = sort_col(sort);

        let sql = format!(
            "SELECT {SELECT_COLS} FROM (\
                SELECT DISTINCT ON (name) * FROM {table} ORDER BY name, cached_at DESC\
            ) sub ORDER BY {col} DESC LIMIT $1 OFFSET $2"
        );

        sqlx::query(&sql)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| row_to_result(r, ecosystem))
            .collect()
    }

    pub async fn alternatives(
        &self,
        ecosystem: &str,
        name: &str,
        limit: i64,
    ) -> Result<Vec<SizeResult>, AppError> {
        let table = ecosystem_table(ecosystem)?;

        let sql = format!(
            "WITH source AS (\
                SELECT keywords FROM {table} WHERE name = $1 ORDER BY cached_at DESC LIMIT 1\
            ), \
            candidates AS (\
                SELECT DISTINCT ON (name) * FROM {table}\
                WHERE name != $1 AND keywords && (SELECT keywords FROM source)\
                ORDER BY name, cached_at DESC\
            )\
            SELECT {SELECT_COLS}, \
                (\
                    SELECT COUNT(*) FROM unnest(candidates.keywords) k\
                    WHERE k = ANY((SELECT keywords FROM source))\
                ) AS overlap\
            FROM candidates\
            ORDER BY overlap DESC, monthly_downloads DESC\
            LIMIT $2"
        );

        sqlx::query(&sql)
            .bind(name)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| row_to_result(r, ecosystem))
            .collect()
    }
}

/// Map ecosystem name to its table. Returns AppError::NotFound for unknown ecosystems
/// so callers surface a clean 404 rather than a SQL error.
fn ecosystem_table(ecosystem: &str) -> Result<&'static str, AppError> {
    match ecosystem {
        "npm" => Ok("npm_packages"),
        other => Err(AppError::NotFound(format!("unknown ecosystem: {other}"))),
    }
}
