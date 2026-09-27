//! Read-only filesystem capacity reporting for the profile chooser.
use crate::{ApiResult, App, internal};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Serialize)]
/// Bytes stored beneath one directory managed by Custom Plex.
struct Location {
    name: String,
    bytes: u64,
}

#[derive(Serialize)]
/// Capacity for one distinct underlying filesystem.
struct Volume {
    name: String,
    total: u64,
    available: u64,
}

#[derive(Serialize)]
/// Aggregate storage response shown on the profile chooser.
struct Storage {
    managed: u64,
    total: u64,
    available: u64,
    locations: Vec<Location>,
    volumes: Vec<Volume>,
}

/// Counts regular files directly in the data directory without including the
/// default nested photo directory a second time.
fn app_data_bytes(path: &Path) -> std::io::Result<u64> {
    let mut bytes = 0_u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            bytes = bytes.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(bytes)
}

/// Counts regular files recursively without following symbolic links.
fn directory_bytes(path: &Path) -> Result<u64, walkdir::Error> {
    let mut bytes = 0_u64;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            bytes = bytes.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(bytes)
}

#[cfg(unix)]
/// Returns the filesystem device ID used to prevent double-counting mounts.
fn device(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(path.metadata()?.dev())
}

#[cfg(not(unix))]
/// Produces a stable per-path fallback where device IDs are unavailable.
fn device(path: &Path) -> std::io::Result<u64> {
    // Capacity remains useful on non-Unix development hosts. Paths are kept
    // distinct because a stable filesystem identifier is unavailable here.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    Ok(hasher.finish())
}

/// Computes capacity off the async runtime because `df` and metadata calls are
/// blocking operations.
async fn usage(State(app): State<App>) -> ApiResult<Json<Storage>> {
    tokio::task::spawn_blocking(move || {
        let mut roots = vec![
            ("App data".to_owned(), app.data.clone()),
            ("Photos".to_owned(), app.photos.clone()),
        ];
        roots.extend(
            app.media
                .iter()
                .map(|(name, path)| (format!("Movies: {name}"), path.clone())),
        );
        let mut locations = Vec::with_capacity(roots.len());
        for (name, path) in &roots {
            let bytes = if name == "App data" {
                app_data_bytes(path).map_err(internal)?
            } else {
                directory_bytes(path).map_err(internal)?
            };
            locations.push(Location {
                name: name.clone(),
                bytes,
            });
        }

        let mut devices = BTreeMap::new();
        let mut volumes = Vec::new();
        for (name, path) in roots {
            let id = device(&path).map_err(internal)?;
            if let Some(index) = devices.get(&id).copied() {
                let volume: &mut Volume = &mut volumes[index];
                volume.name.push_str(" + ");
                volume.name.push_str(&name);
                continue;
            }
            let output = std::process::Command::new("df")
                .args(["-Pk"])
                .arg(&path)
                .output()
                .map_err(internal)?;
            if !output.status.success() {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            let line = String::from_utf8(output.stdout)
                .map_err(internal)?
                .lines()
                .last()
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
                .to_owned();
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 6 {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            let total = fields[1]
                .parse::<u64>()
                .map_err(internal)?
                .saturating_mul(1024);
            let available = fields[3]
                .parse::<u64>()
                .map_err(internal)?
                .saturating_mul(1024);
            volumes.push(Volume {
                name,
                total,
                available,
            });
            devices.insert(id, volumes.len() - 1);
        }
        Ok(Json(Storage {
            managed: locations.iter().map(|location| location.bytes).sum(),
            total: volumes.iter().map(|volume| volume.total).sum(),
            available: volumes.iter().map(|volume| volume.available).sum(),
            locations,
            volumes,
        }))
    })
    .await
    .map_err(internal)?
}

/// Returns the storage-reporting route.
pub fn routes() -> Router<App> {
    Router::new().route("/api/storage", get(usage))
}
