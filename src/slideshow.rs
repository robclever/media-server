//! Shared slideshow presets; album choices do not change the default slideshow selection.
use crate::{ApiResult, App, internal};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

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
/// Creates the additive preset table during application startup.
pub fn initialize(db: &rusqlite::Connection) -> anyhow::Result<()> {
    db.execute("CREATE TABLE IF NOT EXISTS slideshow_presets (id INTEGER PRIMARY KEY, content TEXT NOT NULL)", [])?;
    Ok(())
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
        .route("/api/slideshow/presets", get(list).post(create))
        .route("/api/slideshow/presets/{id}", post(update).delete(delete))
}
