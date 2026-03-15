use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Npm,
    Maven,
    Composer,
    PyPI,
    Cargo,
}

impl std::fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ecosystem::Npm => write!(f, "npm"),
            Ecosystem::Maven => write!(f, "maven"),
            Ecosystem::Composer => write!(f, "composer"),
            Ecosystem::PyPI => write!(f, "pypi"),
            Ecosystem::Cargo => write!(f, "cargo"),
        }
    }
}

impl std::str::FromStr for Ecosystem {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "npm" => Ok(Ecosystem::Npm),
            "maven" => Ok(Ecosystem::Maven),
            "composer" => Ok(Ecosystem::Composer),
            "pypi" => Ok(Ecosystem::PyPI),
            "cargo" => Ok(Ecosystem::Cargo),
            other => Err(format!("unknown ecosystem: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleFormat {
    Esm,
    Cjs,
    Dual,
    Umd,
    Unknown,
}

impl std::str::FromStr for ModuleFormat {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "esm" => ModuleFormat::Esm,
            "cjs" => ModuleFormat::Cjs,
            "dual" => ModuleFormat::Dual,
            "umd" => ModuleFormat::Umd,
            other => {
                tracing::warn!(value = %other, "unrecognised module_format, defaulting to unknown");
                ModuleFormat::Unknown
            }
        })
    }
}

/// The canonical result type. Used as both the internal cache value and the
/// API response — serde attributes produce the correct camelCase JSON shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeResult {
    pub name: String,
    pub version: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub ecosystem: Ecosystem,
    pub size: u64,
    pub gzip: u64,
    pub brotli: u64,
    pub total_size: u64,
    pub total_gzip: u64,
    pub total_brotli: u64,
    pub dependency_count: u32,
    pub dependency_names: Vec<String>,
    pub treeshakeable: bool,
    pub side_effects: bool,
    pub module_format: ModuleFormat,
    pub repository_url: String,
    pub homepage: String,
    pub license: String,
    pub unpacked_size: u64,
    pub has_types: bool,
    pub monthly_downloads: u64,
    pub node_engine: String,
    pub maintainers: Vec<String>,
    pub cached_at: DateTime<Utc>,
}
