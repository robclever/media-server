//! Slideshow selection, playlists, presets, and the browser controller asset.
use crate::{ApiResult, App, internal};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
/// Photo metadata used by the slideshow player.
struct Slide {
    id: i64,
    name: String,
    description: String,
}

#[derive(Deserialize)]
/// Explicit album inclusion setting; omitted or non-boolean values are rejected.
struct SelectionSetting {
    slideshow: bool,
}

#[derive(Deserialize)]
/// Optional explicit album selection; an empty value returns no photos.
struct AlbumQuery {
    albums: Option<String>,
}

#[derive(Deserialize, Serialize)]
/// Validated playback settings stored with a named preset.
pub struct Options {
    interval: String,
    effect: String,
    duration: String,
    shuffle: String,
    captions: String,
    #[serde(default)]
    spotify: String,
}
#[derive(Deserialize, Serialize)]
/// A reusable combination of albums and playback settings.
pub struct Input {
    name: String,
    album_ids: Vec<i64>,
    options: Options,
}
#[derive(Serialize)]
/// Persisted preset returned with its stable identity.
pub struct Preset {
    id: i64,
    #[serde(flatten)]
    input: Input,
}
/// Normalizes names and rejects unsupported settings or untrusted external links.
fn validate(input: &mut Input) -> ApiResult<()> {
    input.name = input.name.trim().to_owned();
    input.album_ids.sort_unstable();
    input.album_ids.dedup();
    let o = &input.options;
    let spotify = o.spotify.is_empty()
        || [
            "https://open.spotify.com/playlist/",
            "https://open.spotify.com/album/",
            "https://open.spotify.com/track/",
        ]
        .iter()
        .any(|prefix| {
            o.spotify
                .strip_prefix(prefix)
                .is_some_and(|id| id.len() == 22 && id.bytes().all(|c| c.is_ascii_alphanumeric()))
        });
    if input.name.is_empty()
        || input.name.chars().count() > 80
        || input.name.chars().any(char::is_control)
        || input.album_ids.len() > 500
        || input.album_ids.iter().any(|id| *id <= 0)
        || !["3", "5", "10", "15", "30"].contains(&o.interval.as_str())
        || !["none", "fade", "slide", "zoom"].contains(&o.effect.as_str())
        || !["300", "600", "1000", "2000"].contains(&o.duration.as_str())
        || !["off", "on"].contains(&o.shuffle.as_str())
        || !["off", "brief", "always"].contains(&o.captions.as_str())
        || !spotify
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}
/// Creates and migrates all slideshow-owned persistence.
pub fn initialize(db: &rusqlite::Connection) -> anyhow::Result<()> {
    let columns = db
        .prepare("PRAGMA table_info(albums)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == "slideshow") {
        db.execute(
            "ALTER TABLE albums ADD COLUMN slideshow INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    db.execute("CREATE TABLE IF NOT EXISTS slideshow_presets (id INTEGER PRIMARY KEY, content TEXT NOT NULL)", [])?;
    Ok(())
}

/// Lists album IDs included by default without exposing slideshow state through the photo API.
async fn selected_albums(State(app): State<App>) -> ApiResult<Json<Vec<i64>>> {
    let db = app.db.lock().unwrap();
    let mut stmt = db
        .prepare("SELECT id FROM albums WHERE slideshow=1 ORDER BY id")
        .map_err(internal)?;
    let rows = stmt.query_map([], |row| row.get(0)).map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}

/// Saves password-free album inclusion, protected by the mutation middleware.
async fn set_selection(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(input): Json<SelectionSetting>,
) -> ApiResult<StatusCode> {
    let changed = app
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE albums SET slideshow=?1 WHERE id=?2",
            rusqlite::params![input.slideshow, id],
        )
        .map_err(internal)?;
    if changed == 0 {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

/// Loads either the default selection or explicitly requested album IDs.
/// Results are ordered by album/photo ID without changing recent use; moves
/// and deletions are reflected on the next request.
async fn playlist(
    State(app): State<App>,
    Query(query): Query<AlbumQuery>,
) -> ApiResult<Json<Vec<Slide>>> {
    let db = app.db.lock().unwrap();
    let ids = query
        .albums
        .as_ref()
        .map(|value| {
            if value.is_empty() {
                return Ok(Vec::new());
            }
            let ids = value
                .split(',')
                .map(|id| id.parse::<i64>().map_err(|_| StatusCode::BAD_REQUEST))
                .collect::<Result<Vec<_>, _>>()?;
            if ids.len() > 500 || ids.iter().any(|id| *id <= 0) {
                return Err(StatusCode::BAD_REQUEST);
            }
            Ok(ids)
        })
        .transpose()?;
    let condition = if let Some(ids) = &ids {
        format!("a.id IN ({})", vec!["?"; ids.len()].join(","))
    } else {
        "a.slideshow=1".to_owned()
    };
    let mut stmt = db.prepare(&format!("SELECT p.id,p.name,p.description FROM photos p JOIN albums a ON a.id=p.album_id WHERE {condition} ORDER BY a.id,p.id")).map_err(internal)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(ids.iter().flatten()), |row| {
            Ok(Slide {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
            })
        })
        .map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}
/// Lists shared presets, including references to albums removed since saving.
async fn list(State(app): State<App>) -> ApiResult<Json<Vec<Preset>>> {
    let db = app.db.lock().unwrap();
    let mut stmt = db
        .prepare("SELECT id,content FROM slideshow_presets ORDER BY id")
        .map_err(internal)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(internal)?;
    let mut presets = Vec::new();
    for row in rows {
        let (id, content) = row.map_err(internal)?;
        presets.push(Preset {
            id,
            input: serde_json::from_str(&content).map_err(internal)?,
        });
    }
    Ok(Json(presets))
}
/// Saves a new preset without changing global album inclusion flags.
async fn create(
    State(app): State<App>,
    Json(mut input): Json<Input>,
) -> ApiResult<(StatusCode, Json<Preset>)> {
    validate(&mut input)?;
    let content = serde_json::to_string(&input).map_err(internal)?;
    let db = app.db.lock().unwrap();
    db.execute(
        "INSERT INTO slideshow_presets(content) VALUES(?1)",
        [content],
    )
    .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(Preset {
            id: db.last_insert_rowid(),
            input,
        }),
    ))
}
/// Replaces an existing preset; callers must explicitly choose Update.
async fn update(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(mut input): Json<Input>,
) -> ApiResult<StatusCode> {
    validate(&mut input)?;
    let content = serde_json::to_string(&input).map_err(internal)?;
    let count = app
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE slideshow_presets SET content=?1 WHERE id=?2",
            rusqlite::params![content, id],
        )
        .map_err(internal)?;
    if count == 0 {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
/// Removes only preset metadata, never albums or photos.
async fn delete(State(app): State<App>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let count = app
        .db
        .lock()
        .unwrap()
        .execute("DELETE FROM slideshow_presets WHERE id=?1", [id])
        .map_err(internal)?;
    if count == 0 {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
/// Password-free routes using the application's existing mutation protection.
pub fn routes() -> Router<App> {
    Router::new()
        .route(
            "/slideshow.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/slideshow.js"),
                )
            }),
        )
        .route("/api/slideshow", get(playlist))
        .route("/api/slideshow/albums", get(selected_albums))
        .route("/api/albums/{id}/slideshow", post(set_selection))
        .route("/api/slideshow/presets", get(list).post(create))
        .route("/api/slideshow/presets/{id}", post(update).delete(delete))
}
