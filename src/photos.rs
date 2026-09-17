//! Album metadata lives in SQLite; originals and browsing images live on PHOTO_DIR.
use crate::{ApiResult, App, internal};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{StatusCode, header},
    response::Response,
};
use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder,
};
use rand_core::{OsRng, RngCore};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::PathBuf};
use tower_http::services::ServeFile;

pub const MAX_UPLOAD: usize = 24 * 1024 * 1024;
#[derive(Serialize)]
pub struct Album {
    id: i64,
    name: String,
    count: i64,
    cover_id: Option<i64>,
}
#[derive(Serialize)]
pub struct Photo {
    id: i64,
    name: String,
}
#[derive(Deserialize)]
pub struct NewAlbum {
    name: String,
}
#[derive(Deserialize)]
pub struct Upload {
    name: String,
}

pub async fn albums(State(app): State<App>) -> ApiResult<Json<Vec<Album>>> {
    let db = app.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT a.id,a.name,COUNT(p.id),MIN(p.id) FROM albums a LEFT JOIN photos p ON p.album_id=a.id GROUP BY a.id ORDER BY a.id DESC").map_err(internal)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Album {
                id: r.get(0)?,
                name: r.get(1)?,
                count: r.get(2)?,
                cover_id: r.get(3)?,
            })
        })
        .map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}
pub async fn create(
    State(app): State<App>,
    Json(input): Json<NewAlbum>,
) -> ApiResult<(StatusCode, Json<Album>)> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let db = app.db.lock().unwrap();
    db.execute("INSERT INTO albums(name) VALUES(?1)", [name])
        .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(Album {
            id: db.last_insert_rowid(),
            name: name.into(),
            count: 0,
            cover_id: None,
        }),
    ))
}
fn exists(app: &App, id: i64) -> ApiResult<()> {
    let found: bool = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM albums WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if found {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
pub async fn list(State(app): State<App>, Path(id): Path<i64>) -> ApiResult<Json<Vec<Photo>>> {
    exists(&app, id)?;
    let db = app.db.lock().unwrap();
    let mut stmt = db
        .prepare("SELECT id,name FROM photos WHERE album_id=?1 ORDER BY id")
        .map_err(internal)?;
    let rows = stmt
        .query_map([id], |r| {
            Ok(Photo {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}
fn prepare(bytes: &[u8]) -> anyhow::Result<(&'static str, Vec<u8>, Vec<u8>)> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let extension = match reader.format() {
        Some(ImageFormat::Jpeg) => "jpg",
        Some(ImageFormat::Png) => "png",
        Some(ImageFormat::WebP) => "webp",
        _ => anyhow::bail!("Unsupported image"),
    };
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut picture = DynamicImage::from_decoder(decoder)?;
    picture.apply_orientation(orientation);
    let mut preview = Vec::new();
    let mut thumb = Vec::new();
    JpegEncoder::new_with_quality(&mut preview, 88).encode_image(
        &picture
            .thumbnail(picture.width().min(1600), picture.height().min(1600))
            .to_rgb8(),
    )?;
    JpegEncoder::new_with_quality(&mut thumb, 80).encode_image(
        &picture
            .thumbnail(picture.width().min(400), picture.height().min(400))
            .to_rgb8(),
    )?;
    Ok((extension, preview, thumb))
}
pub async fn upload(
    State(app): State<App>,
    Path(id): Path<i64>,
    Query(input): Query<Upload>,
    bytes: Bytes,
) -> ApiResult<(StatusCode, Json<Photo>)> {
    exists(&app, id)?;
    let name = input.name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 255 || name.chars().any(char::is_control) {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Share a single expensive-image slot with movie covers on small Pis.
    let permit = app
        .cover_work
        .clone()
        .acquire_owned()
        .await
        .map_err(internal)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let (extension, preview, thumb) =
            prepare(&bytes).map_err(|_| StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
        let root = app.photos.canonicalize().map_err(internal)?;
        let mut random = [0u8; 16];
        OsRng.fill_bytes(&mut random);
        let token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let staging = root.join(format!(".upload-{token}"));
        let destination = root.join(&token);
        std::fs::create_dir(&staging).map_err(internal)?;
        let stored = (|| -> std::io::Result<()> {
            std::fs::write(staging.join(format!("original.{extension}")), &bytes)?;
            std::fs::write(staging.join("preview.jpg"), preview)?;
            std::fs::write(staging.join("thumbnail.jpg"), thumb)?;
            std::fs::rename(&staging, &destination)
        })();
        if let Err(error) = stored {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(internal(error));
        }
        let db = app.db.lock().unwrap();
        if let Err(error) = db.execute(
            "INSERT INTO photos(album_id,name,storage,extension) VALUES(?1,?2,?3,?4)",
            params![id, name, token, extension],
        ) {
            let _ = std::fs::remove_dir_all(destination);
            return Err(internal(error));
        }
        Ok((
            StatusCode::CREATED,
            Json(Photo {
                id: db.last_insert_rowid(),
                name,
            }),
        ))
    })
    .await
    .map_err(internal)?
}
pub async fn file(
    State(app): State<App>,
    Path((id, variant)): Path<(i64, String)>,
    request: Request,
) -> ApiResult<Response> {
    if !["original", "preview", "thumbnail"].contains(&variant.as_str()) {
        return Err(StatusCode::NOT_FOUND);
    }
    let (storage, extension): (String, String) = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT storage,extension FROM photos WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let root = tokio::fs::canonicalize(&app.photos)
        .await
        .map_err(internal)?;
    let name = if variant == "original" {
        format!("original.{extension}")
    } else {
        format!("{variant}.jpg")
    };
    let path: PathBuf = tokio::fs::canonicalize(root.join(storage).join(name))
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !path.starts_with(&root) {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut response = ServeFile::new(path)
        .try_call(request)
        .await
        .map_err(internal)?
        .map(axum::body::Body::new);
    if variant == "original" {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, "attachment".parse().unwrap());
    }
    Ok(response)
}

pub fn initialize(db: &rusqlite::Connection, data: &std::path::Path) -> anyhow::Result<PathBuf> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS albums (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS photos (id INTEGER PRIMARY KEY, album_id INTEGER NOT NULL REFERENCES albums(id), name TEXT NOT NULL, storage TEXT NOT NULL UNIQUE, extension TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS photos_album ON photos(album_id);")?;
    let photos = data.join("photo-albums");
    std::fs::create_dir_all(&photos)?;
    Ok(photos)
}
impl App {
    pub fn with_photo_directory(mut self, path: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&path)?;
        self.photos = path.canonicalize()?;
        Ok(self)
    }
}
pub fn routes() -> axum::Router<App> {
    use axum::{Router, routing::get};
    Router::new()
        .route(
            "/photos.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/photos.js"),
                )
            }),
        )
        .route("/api/albums", get(albums).post(create))
        .route(
            "/api/albums/{id}/photos",
            get(list)
                .post(upload)
                .layer(axum::extract::DefaultBodyLimit::max(MAX_UPLOAD)),
        )
        .route("/api/photos/{id}/{variant}", get(file))
}
