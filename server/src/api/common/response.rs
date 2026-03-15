use serde::Serialize;

use crate::common::types::SizeResult;

/// 200 — result ready
#[derive(Serialize)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub result: SizeResult,
}

/// 202 — processing / queued
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedResponse {
    pub status: &'static str,
    pub name: String,
    pub version: String,
    pub retry_after: u32,
}

/// 422 — analysis failed
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedResponse {
    pub status: &'static str,
    pub name: String,
    pub version: String,
    pub error: String,
}

/// Bundlephobia-compatible response (subset of fields)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlephobiaResponse {
    pub name: String,
    pub version: String,
    pub size: u64,
    pub gzip: u64,
}

impl From<SizeResult> for BundlephobiaResponse {
    fn from(r: SizeResult) -> Self {
        Self { name: r.name, version: r.version, size: r.size, gzip: r.gzip }
    }
}
