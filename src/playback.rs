use crate::{ApiResult, App, internal, now};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::params;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Progress {
    position: f64,
    duration: f64,
}

pub fn initialize(db: &rusqlite::Connection) -> anyhow::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS playback_progress (
            movie_id INTEGER PRIMARY KEY REFERENCES movies(id) ON DELETE CASCADE,
            position REAL NOT NULL,
            duration REAL NOT NULL,
            updated_at INTEGER NOT NULL
        );",
    )?;
    Ok(())
}

async fn save(
    State(app): State<App>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<Progress>,
) -> ApiResult<StatusCode> {
    if !input.position.is_finite()
        || !input.duration.is_finite()
        || input.position < 0.0
        || input.duration <= 0.0
        || input.duration > 604_800.0
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let parent = app.parent(&headers);
    let db = app.db.lock().unwrap();
    let visible: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM movies WHERE id=?1 AND present=1 AND (approved=1 OR ?2))",
            params![id, parent],
            |row| row.get(0),
        )
        .map_err(internal)?;
    if !visible {
        return Err(StatusCode::NOT_FOUND);
    }
    // Treat the opening seconds and the final 15 seconds as completed/reset.
    if input.position <= 1.0 || input.duration - input.position.min(input.duration) <= 15.0 {
        db.execute("DELETE FROM playback_progress WHERE movie_id=?1", [id])
            .map_err(internal)?;
    } else {
        db.execute(
            "INSERT INTO playback_progress(movie_id,position,duration,updated_at) VALUES(?1,?2,?3,?4)
             ON CONFLICT(movie_id) DO UPDATE SET position=excluded.position,duration=excluded.duration,updated_at=excluded.updated_at",
            params![id, input.position.min(input.duration), input.duration, now()],
        )
        .map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> Router<App> {
    Router::new().route("/api/movies/{id}/progress", post(save))
}
