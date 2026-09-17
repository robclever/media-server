//! Covers are independently authorized on every request, including cached images.
use crate::{ApiResult, App, internal};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use image::{ImageReader, Limits, codecs::jpeg::JpegEncoder};
use rusqlite::{OptionalExtension, params};
use std::{
    io::Cursor,
    path::{Path as FsPath, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
use tokio::io::AsyncReadExt;

pub const MAX_UPLOAD: usize = 8 * 1024 * 1024;
const UPLOADED: &str = "uploaded";

fn location(app: &App, id: i64, headers: &HeaderMap) -> ApiResult<(PathBuf, PathBuf)> {
    let parent = app.parent(headers);
    let (source, relative): (String, String) = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT source,path FROM movies WHERE id=?1 AND present=1 AND (approved=1 OR ?2)",
            params![id, parent],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let root = app.media.get(&source).ok_or(StatusCode::NOT_FOUND)?.clone();
    Ok((root.clone(), root.join(relative)))
}

fn cached(app: &App, id: i64) -> ApiResult<Option<(String, Vec<u8>)>> {
    app.db
        .lock()
        .unwrap()
        .query_row(
            "SELECT fingerprint,jpeg FROM covers WHERE movie_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(internal)
}
fn save(app: &App, id: i64, fingerprint: &str, bytes: &[u8]) -> ApiResult<()> {
    app.db.lock().unwrap().execute("INSERT INTO covers VALUES(?1,?2,?3) ON CONFLICT(movie_id) DO UPDATE SET fingerprint=excluded.fingerprint,jpeg=excluded.jpeg",params![id,fingerprint,bytes]).map_err(internal)?;
    Ok(())
}
fn response(bytes: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response()
}

fn normalize(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader.decode()?.thumbnail(640, 640).to_rgb8();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&picture)?;
    Ok(jpeg)
}

pub async fn upload(
    State(app): State<App>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<StatusCode> {
    location(&app, id, &headers)?;
    // Bound image decoding and frame extraction together on small Raspberry Pis.
    let _permit = app.cover_work.acquire().await.map_err(internal)?;
    let jpeg = tokio::task::spawn_blocking(move || normalize(&bytes))
        .await
        .map_err(internal)?
        .map_err(|_| StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    location(&app, id, &headers)?;
    save(&app, id, UPLOADED, &jpeg)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reset(
    State(app): State<App>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult<StatusCode> {
    location(&app, id, &headers)?;
    let _permit = app.cover_work.acquire().await.map_err(internal)?;
    location(&app, id, &headers)?;
    app.db
        .lock()
        .unwrap()
        .execute("DELETE FROM covers WHERE movie_id=?1", [id])
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn fingerprint(path: &FsPath) -> ApiResult<String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let modified = meta
        .modified()
        .map_err(internal)?
        .duration_since(UNIX_EPOCH)
        .map_err(internal)?
        .as_nanos();
    Ok(format!("{}:{}:{modified}", path.display(), meta.len()))
}

async fn read_image(path: &FsPath) -> anyhow::Result<Vec<u8>> {
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::new();
    file.take((MAX_UPLOAD + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    anyhow::ensure!(bytes.len() <= MAX_UPLOAD, "Image too large");
    tokio::task::spawn_blocking(move || normalize(&bytes)).await?
}

async fn frame(path: &FsPath) -> Option<Vec<u8>> {
    // Short clips may have no frame at 10 seconds; retry at the start.
    for position in ["10", "0"] {
        let mut command = tokio::process::Command::new("ffmpeg");
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-max_alloc",
                "67108864",
                "-protocol_whitelist",
                "file,pipe",
                "-threads",
                "1",
                "-ss",
                position,
                "-i",
            ])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-an",
                "-vf",
                "scale=640:640:force_original_aspect_ratio=decrease",
                "-threads",
                "1",
                "-f",
                "image2pipe",
                "-c:v",
                "mjpeg",
                "pipe:1",
            ])
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(20), command.output()).await;
        if let Ok(Ok(output)) = output {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(output.stdout);
            }
        } else {
            break;
        }
    }
    None
}

pub async fn get(
    State(app): State<App>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let (root, video) = location(&app, id, &headers)?;
    let _permit = app.cover_work.acquire().await.map_err(internal)?;
    location(&app, id, &headers)?;
    if let Some((key, jpeg)) = cached(&app, id)?
        && key == UPLOADED
    {
        return Ok(response(jpeg));
    }
    let video = tokio::fs::canonicalize(video)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !video.starts_with(&root) {
        return Err(StatusCode::NOT_FOUND);
    }
    // Existing same-name artwork wins over video frame extraction.
    for extension in ["jpg", "jpeg", "png", "webp"] {
        let candidate = video.with_extension(extension);
        let Ok(candidate) = tokio::fs::canonicalize(candidate).await else {
            continue;
        };
        if !candidate.starts_with(&root) {
            continue;
        }
        let key = format!("image:{}", fingerprint(&candidate).await?);
        if let Some((cached_key, jpeg)) = cached(&app, id)?
            && cached_key == key
        {
            return Ok(response(jpeg));
        }
        if let Ok(jpeg) = read_image(&candidate).await {
            location(&app, id, &headers)?;
            save(&app, id, &key, &jpeg)?;
            return Ok(response(jpeg));
        }
    }
    let key = format!("frame:{}", fingerprint(&video).await?);
    if let Some((cached_key, jpeg)) = cached(&app, id)?
        && cached_key == key
    {
        return if jpeg.is_empty() {
            Err(StatusCode::NOT_FOUND)
        } else {
            Ok(response(jpeg))
        };
    }
    let jpeg = frame(&video).await.unwrap_or_default();
    location(&app, id, &headers)?;
    // Cache failures too: a broken file should not launch FFmpeg on every page view.
    save(&app, id, &key, &jpeg)?;
    if jpeg.is_empty() {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(response(jpeg))
    }
}
