mod svg;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::{Router, routing::get};
use serde::Deserialize;

use super::AppState;
use crate::common::types::SizeResult;

#[derive(Debug, Deserialize)]
pub struct BadgeParams {
    metric: Option<String>,
    style: Option<String>,
    color: Option<String>,
    label: Option<String>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/badge/{ecosystem}/{*rest}", get(handle_badge))
        .with_state(state)
}

async fn handle_badge(
    State(state): State<AppState>,
    Path((ecosystem, rest)): Path<(String, String)>,
    Query(params): Query<BadgeParams>,
) -> Response {
    // rest = "react.svg" or "@scope/package.svg"
    let package = match rest.strip_suffix(".svg") {
        Some(name) => name,
        None => return badge_error("not found", "flat"),
    };

    if package.is_empty() {
        return badge_error("not found", "flat");
    }

    let style = params.style.as_deref().unwrap_or("flat");

    let result = match state.cache.get_latest(&ecosystem, package).await {
        Ok(Some(r)) => r,
        Ok(None) => return badge_error("not found", style),
        Err(_) => return badge_error("error", style),
    };

    state.cache.track_request(&ecosystem, package);

    let metric = params.metric.as_deref().unwrap_or("gzip");
    let (lbl, val, default_color) = metric_label_value(metric, &result);

    let label = params.label.as_deref().unwrap_or(&lbl);
    let color = params.color.as_deref().unwrap_or(&default_color);

    // Validate color: # + hex, or named
    let color = sanitize_color(color);

    let body = svg::render(label, &val, &color, style);
    svg_response(body)
}

fn metric_label_value(metric: &str, r: &SizeResult) -> (String, String, String) {
    match metric {
        "size" | "minified" => {
            let v = svg::format_bytes(r.size);
            ("minified".into(), v, size_color(r.size))
        }
        "gzip" => {
            let v = svg::format_bytes(r.gzip);
            ("gzip".into(), v, size_color(r.gzip))
        }
        "brotli" => {
            let v = svg::format_bytes(r.brotli);
            ("brotli".into(), v, size_color(r.brotli))
        }
        "treeshakeable" | "tree-shaking" => {
            let (v, c) = if r.treeshakeable {
                ("\u{2713}".to_string(), "#44cc11")
            } else {
                ("\u{2717}".to_string(), "#9f9f9f")
            };
            ("tree-shaking".into(), v, c.into())
        }
        "side-effects" | "sideEffects" => {
            let (v, c) = if r.side_effects {
                ("yes".to_string(), "#dfb317")
            } else {
                ("none".to_string(), "#44cc11")
            };
            ("side effects".into(), v, c.into())
        }
        "module" | "moduleFormat" => {
            let v = format!("{}", serde_json::to_value(&r.module_format)
                .and_then(|v| Ok(v.as_str().unwrap_or("unknown").to_string()))
                .unwrap_or_else(|_| "unknown".into()));
            let c = match v.as_str() {
                "esm" => "#44cc11",
                "dual" => "#007ec6",
                _ => "#dfb317",
            };
            ("module".into(), v, c.into())
        }
        "dependencies" | "deps" => {
            let v = r.dependency_count.to_string();
            let c = match r.dependency_count {
                0 => "#44cc11",
                1..=5 => "#007ec6",
                6..=20 => "#dfb317",
                _ => "#e05d44",
            };
            ("deps".into(), v, c.into())
        }
        "types" => {
            let (v, c) = if r.has_types {
                ("included".to_string(), "#44cc11")
            } else {
                ("missing".to_string(), "#9f9f9f")
            };
            ("types".into(), v, c.into())
        }
        "license" => {
            let v = if r.license.is_empty() { "unknown" } else { &r.license };
            ("license".into(), v.to_string(), "#007ec6".into())
        }
        "downloads" => {
            let v = svg::format_downloads(r.monthly_downloads);
            ("downloads".into(), format!("{}/month", v), "#007ec6".into())
        }
        "version" => {
            ("version".into(), r.version.clone(), "#007ec6".into())
        }
        _ => {
            // Default to gzip
            let v = svg::format_bytes(r.gzip);
            ("gzip".into(), v, size_color(r.gzip))
        }
    }
}

/// Color based on gzip/minified size thresholds.
fn size_color(bytes: u64) -> String {
    match bytes {
        0..=5_000 => "#44cc11",      // green
        5_001..=25_000 => "#dfb317", // yellow
        25_001..=100_000 => "#fe7d37", // orange
        _ => "#e05d44",              // red
    }
    .into()
}

fn badge_error(text: &str, style: &str) -> Response {
    let body = svg::render("shipweight", text, "#9f9f9f", style);
    svg_response(body)
}

fn svg_response(body: String) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400, s-maxage=86400"),
        ],
        body,
    )
        .into_response()
}

/// Only allow valid CSS color values to prevent injection.
fn sanitize_color(color: &str) -> String {
    // Accept: #hex (3, 4, 6, or 8 digits), or simple named colors
    let trimmed = color.trim();
    if trimmed.starts_with('#') && trimmed.len() <= 9 && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        return trimmed.to_string();
    }
    // Named colors (subset of common ones)
    match trimmed {
        "green" | "blue" | "red" | "orange" | "yellow" | "grey" | "gray"
        | "brightgreen" | "yellowgreen" | "lightgrey" | "success" | "important"
        | "critical" | "informational" | "inactive" => {
            named_to_hex(trimmed).to_string()
        }
        _ => "#007ec6".to_string(), // fallback: blue
    }
}

fn named_to_hex(name: &str) -> &str {
    match name {
        "green" | "success" | "brightgreen" => "#44cc11",
        "yellowgreen" => "#a4a61d",
        "yellow" => "#dfb317",
        "orange" | "important" => "#fe7d37",
        "red" | "critical" => "#e05d44",
        "blue" | "informational" => "#007ec6",
        "grey" | "gray" | "lightgrey" | "inactive" => "#9f9f9f",
        _ => "#007ec6",
    }
}
