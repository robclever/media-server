//! Read-only filesystem capacity reporting for the profile chooser.
use crate::{ApiResult, App, internal};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;
use std::{collections::BTreeSet, path::Path};

#[derive(Serialize)]
struct Volume {
    name: String,
    total: u64,
    available: u64,
}

#[derive(Serialize)]
struct Storage {
    total: u64,
    available: u64,
    volumes: Vec<Volume>,
}

#[cfg(unix)]
fn device(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(path.metadata()?.dev())
}

#[cfg(not(unix))]
fn device(path: &Path) -> std::io::Result<u64> {
    // Capacity remains useful on non-Unix development hosts. Paths are kept
    // distinct because a stable filesystem identifier is unavailable here.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    Ok(hasher.finish())
}

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
        let mut seen = BTreeSet::new();
        let mut volumes = Vec::new();
        for (name, path) in roots {
            let id = device(&path).map_err(internal)?;
            if !seen.insert(id) {
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
        }
        Ok(Json(Storage {
            total: volumes.iter().map(|volume| volume.total).sum(),
            available: volumes.iter().map(|volume| volume.available).sum(),
            volumes,
        }))
    })
    .await
    .map_err(internal)?
}

pub fn routes() -> Router<App> {
    Router::new().route("/api/storage", get(usage))
}
