//! Server-side directory listing backing the JS "Browse…" picker (there is no
//! native file dialog in a browser, and this app has full filesystem access
//! on the user's own machine).

use std::fs;
use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct BrowseQuery {
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
pub struct Entry {
    name: String,
    path: String,
    is_dir: bool,
}

#[derive(Serialize)]
pub struct BrowseResponse {
    path: String,
    parent: Option<String>,
    entries: Vec<Entry>,
}

pub async fn browse(
    Query(query): Query<BrowseQuery>,
) -> Result<Json<BrowseResponse>, (StatusCode, String)> {
    let requested = query.path.trim();
    let base = if requested.is_empty() {
        home_dir()
    } else {
        PathBuf::from(requested)
    };
    let dir = if base.is_dir() {
        base
    } else {
        base.parent()
            .map(Path::to_path_buf)
            .filter(|p| p.is_dir())
            .unwrap_or_else(home_dir)
    };
    let dir = dir.canonicalize().unwrap_or(dir);

    let read_dir = fs::read_dir(&dir)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Could not read {}: {e}", dir.display())))?;

    let mut entries: Vec<Entry> = read_dir
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            let is_dir = path.is_dir();
            Some(Entry {
                name,
                path: path.display().to_string(),
                is_dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let parent = dir.parent().map(|p| p.display().to_string());
    Ok(Json(BrowseResponse {
        path: dir.display().to_string(),
        parent,
        entries,
    }))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}
