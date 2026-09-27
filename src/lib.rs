//! Core library for the Family Cinema media server.
//!
//! [`App`] owns the SQLite connection, configured media roots, persistent data
//! directory, photo storage, authentication state, and Raspberry Pi work
//! limits. [`router`] exposes that state through the HTTP interface. The binary
//! in `main.rs` is deliberately small: it translates environment variables
//! into an [`App`], scans the library, and starts Axum.
//!
//! # Persistence
//!
//! User accounts, sessions, movie metadata, approvals, covers, playback
//! progress, albums, and photo metadata live in `library.sqlite3`. Movie files
//! remain in read-only media roots. Photo originals and generated previews live
//! in the writable photo directory.
#![warn(missing_docs)]

mod covers;
mod photos;
mod playback;
mod storage;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tower_http::services::ServeFile;

type ApiResult<T> = Result<T, StatusCode>;
#[derive(Clone)]
/// Shared application state used by every HTTP handler.
///
/// Clones are inexpensive because mutable state is reference counted. Open an
/// instance with [`App::open`] or [`App::open_sources`], optionally replace its
/// photo directory with [`App::with_photo_directory`], then pass it to
/// [`router`].
pub struct App {
    db: Arc<Mutex<Connection>>,
    media: BTreeMap<String, PathBuf>,
    data: PathBuf,
    secure: bool,
    photos: PathBuf,
    attempts: Arc<Mutex<Vec<i64>>>,
    cover_work: Arc<tokio::sync::Semaphore>,
}
/// Returns Unix time in whole seconds for sessions and rate limiting.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
/// Converts internal implementation failures into a non-disclosing HTTP error.
fn internal(_: impl std::fmt::Display) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}
impl App {
    /// Opens an application with one media root named `default`.
    ///
    /// `media` must already exist. `data` is created when necessary and holds
    /// `library.sqlite3` plus the default photo directory. `secure` controls
    /// whether login cookies include the HTTPS-only `Secure` attribute.
    pub fn open(media: PathBuf, data: PathBuf, secure: bool) -> anyhow::Result<Self> {
        Self::open_sources(BTreeMap::from([("default".into(), media)]), data, secure)
    }
    /// Opens an application with multiple named media roots.
    ///
    /// Source names become stable parts of movie identity and may contain only
    /// ASCII letters, numbers, hyphens, and underscores. Roots are
    /// canonicalized and may not overlap. Opening performs additive database
    /// migrations but does not scan the filesystem; call [`App::scan`] after
    /// construction.
    pub fn open_sources(
        mut media: BTreeMap<String, PathBuf>,
        data: PathBuf,
        secure: bool,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !media.is_empty(),
            "MEDIA_SOURCES must contain at least one location"
        );
        for (name, path) in &mut media {
            anyhow::ensure!(
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "Source names must use letters, numbers, hyphens or underscores"
            );
            *path = path.canonicalize().map_err(|e| {
                anyhow::anyhow!("Cannot open media source {name} ({}): {e}", path.display())
            })?;
            anyhow::ensure!(path.is_dir(), "Media source {name} must be a directory");
        }
        let roots: Vec<_> = media.values().collect();
        for (i, root) in roots.iter().enumerate() {
            for other in &roots[i + 1..] {
                anyhow::ensure!(
                    !root.starts_with(other) && !other.starts_with(root),
                    "Media sources must not overlap or duplicate each other"
                );
            }
        }
        std::fs::create_dir_all(&data)?;
        let data = data.canonicalize()?;
        let mut db = Connection::open(data.join("library.sqlite3"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS account (id INTEGER PRIMARY KEY CHECK(id=1), hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sessions (token TEXT PRIMARY KEY, expires INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS movies (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, title TEXT NOT NULL, approved INTEGER NOT NULL DEFAULT 0, present INTEGER NOT NULL DEFAULT 1);")?;
        // Rebuild the legacy unique-path table atomically, retaining movie IDs and approvals.
        let has_source: bool = db
            .prepare("PRAGMA table_info(movies)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == "source");
        if !has_source {
            let tx = db.transaction()?;
            tx.execute_batch("ALTER TABLE movies RENAME TO movies_legacy;
                CREATE TABLE movies (id INTEGER PRIMARY KEY, source TEXT NOT NULL, path TEXT NOT NULL, title TEXT NOT NULL, approved INTEGER NOT NULL DEFAULT 0, present INTEGER NOT NULL DEFAULT 1, UNIQUE(source,path));
                INSERT INTO movies SELECT id,'default',path,title,approved,present FROM movies_legacy;
                DROP TABLE movies_legacy;")?;
            tx.commit()?;
        }
        db.execute_batch("CREATE TABLE IF NOT EXISTS covers (movie_id INTEGER PRIMARY KEY, fingerprint TEXT NOT NULL, jpeg BLOB NOT NULL);")?;
        playback::initialize(&db)?;
        let photos = photos::initialize(&db, &data)?;
        // Removed sources must never remain visible between startup and the first scan.
        let tx = db.transaction()?;
        let known = media.keys().cloned().collect::<Vec<_>>();
        let mut stmt = tx.prepare("SELECT DISTINCT source FROM movies")?;
        let stored = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for source in stored {
            if !known.contains(&source) {
                tx.execute("UPDATE movies SET present=0 WHERE source=?1", [source])?;
            }
        }
        tx.commit()?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            media,
            data,
            secure,
            photos,
            attempts: Arc::new(Mutex::new(Vec::new())),
            cover_work: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
    /// Hashes and stores the Parents password and revokes every active session.
    ///
    /// Passwords shorter than twelve bytes are rejected. Only the Argon2 hash
    /// is persisted.
    pub fn set_password(&self, password: &str) -> anyhow::Result<()> {
        anyhow::ensure!(password.len() >= 12, "Use at least 12 characters");
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .to_string();
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO account VALUES(1, ?1) ON CONFLICT(id) DO UPDATE SET hash=excluded.hash",
            [&hash],
        )?;
        tx.execute("DELETE FROM sessions", [])?;
        tx.commit()?;
        Ok(())
    }
    /// Recursively indexes supported video files in all configured roots.
    ///
    /// Existing display titles and Baby approvals are preserved for the same
    /// `(source, relative path)` identity. Missing files are marked absent but
    /// retained so their metadata returns if the path reappears.
    pub fn scan(&self) -> anyhow::Result<usize> {
        let mut files = Vec::new();
        for (source, root) in &self.media {
            for entry in walkdir::WalkDir::new(root).follow_links(false) {
                let entry = entry?;
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !["mp4", "m4v", "webm", "mov", "mkv"].contains(&ext.as_str()) {
                    continue;
                }
                files.push((
                    source.clone(),
                    path.strip_prefix(root)?.to_string_lossy().into_owned(),
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .replace(['_', '.'], " "),
                ));
            }
        }
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        tx.execute("UPDATE movies SET present=0", [])?;
        for (source, path, title) in &files {
            tx.execute("INSERT INTO movies(source,path,title) VALUES(?1,?2,?3) ON CONFLICT(source,path) DO UPDATE SET present=1", params![source,path,title])?;
        }
        tx.commit()?;
        Ok(files.len())
    }
    /// Checks whether the request carries an unexpired Parents session.
    fn parent(&self, headers: &HeaderMap) -> bool {
        let Some(token) = cookie(headers) else {
            return false;
        };
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE token=?1 AND expires>?2)",
                params![token, now()],
                |r| r.get::<_, bool>(0),
            )
            .unwrap_or(false)
    }
}
/// Extracts the application session token from a Cookie header.
fn cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| s.trim().strip_prefix("session="))
}
/// Applies the state-change header check and common browser security headers.
async fn protections(req: Request, next: Next) -> Response {
    if req.method() != axum::http::Method::GET
        && req.method() != axum::http::Method::HEAD
        && req
            .headers()
            .get("x-requested-with")
            .and_then(|v| v.to_str().ok())
            != Some("custom-plex")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(req).await;
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("content-security-policy", HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; media-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"));
    response
}
/// Builds the complete HTTP router, including embedded web assets and APIs.
///
/// State-changing requests are protected by the `X-Requested-With:
/// custom-plex` header middleware. Individual handlers still enforce their own
/// Parents or visibility rules.
pub fn router(app: App) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../web/style.css"),
                )
            }),
        )
        .route(
            "/controls.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../web/controls.css"),
                )
            }),
        )
        .merge(photos::routes())
        .merge(playback::routes())
        .merge(storage::routes())
        .route("/health", get(|| async { "ok" }))
        .route("/api/session", get(session))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/movies", get(movies))
        .route("/api/scan", post(scan))
        .route("/api/movies/{id}/approval", post(approve))
        .route("/api/movies/{id}/title", post(rename_movie))
        .route("/media/{id}", get(stream))
        .route(
            "/api/movies/{id}/cover",
            get(covers::get)
                .post(covers::upload)
                .delete(covers::reset)
                .layer(axum::extract::DefaultBodyLimit::max(covers::MAX_UPLOAD)),
        )
        .layer(middleware::from_fn(protections))
        .with_state(app)
}
#[derive(Serialize)]
struct Session {
    parent: bool,
}
/// Reports whether the current browser has a valid Parents session.
async fn session(State(app): State<App>, headers: HeaderMap) -> Json<Session> {
    Json(Session {
        parent: app.parent(&headers),
    })
}
#[derive(Deserialize)]
struct Login {
    password: String,
}
/// Verifies the Parents password and creates an eight-hour session cookie.
async fn login(State(app): State<App>, Json(input): Json<Login>) -> ApiResult<Response> {
    {
        let mut attempts = app.attempts.lock().unwrap();
        attempts.retain(|t| now().saturating_sub(*t) < 60);
        if attempts.len() >= 10 {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        attempts.push(now());
    }
    let hash: Option<String> = app
        .db
        .lock()
        .unwrap()
        .query_row("SELECT hash FROM account WHERE id=1", [], |r| r.get(0))
        .ok();
    let hash = hash.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let verified_hash = hash.clone();
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(input.password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .map_err(internal)?;
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    {
        let db = app.db.lock().unwrap();
        db.execute("DELETE FROM sessions WHERE expires<=?1", [now()])
            .map_err(internal)?;
        // Atomically reject a login if the password was reset while hashing.
        let inserted = db.execute(
            "INSERT INTO sessions SELECT ?1, ?2 WHERE EXISTS(SELECT 1 FROM account WHERE id=1 AND hash=?3)",
            params![token, now() + 28800, verified_hash],
        ).map_err(internal)?;
        if inserted == 0 {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    let cookie = format!(
        "session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=28800{}",
        if app.secure { "; Secure" } else { "" }
    );
    Ok(([(header::SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response())
}
/// Deletes the current server-side session and expires its browser cookie.
async fn logout(State(app): State<App>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = cookie(&headers) {
        app.db
            .lock()
            .unwrap()
            .execute("DELETE FROM sessions WHERE token=?1", [token])
            .map_err(internal)?;
    }
    Ok((
        [(
            header::SET_COOKIE,
            "session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}
#[derive(Serialize, Deserialize)]
/// Movie metadata returned by `GET /api/movies`.
pub struct Movie {
    /// Stable SQLite identifier used by media, cover, and progress routes.
    pub id: i64,
    /// Editable catalog title.
    pub title: String,
    /// Configured media-source name containing the file.
    pub source: String,
    /// Whether anonymous Baby users may see and stream the movie.
    pub approved: bool,
    /// Shared resume position in seconds, when one is saved.
    pub position: Option<f64>,
    /// Duration reported by the browser when progress was last saved.
    pub duration: Option<f64>,
}
/// Lists all present movies visible to the current profile with resume state.
async fn movies(State(app): State<App>, headers: HeaderMap) -> ApiResult<Json<Vec<Movie>>> {
    let parent = app.parent(&headers);
    let db = app.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT m.id,m.title,m.approved,m.source,p.position,p.duration FROM movies m LEFT JOIN playback_progress p ON p.movie_id=m.id WHERE m.present=1 AND (m.approved=1 OR ?1) ORDER BY m.title COLLATE NOCASE").map_err(internal)?;
    let rows = stmt
        .query_map([parent], |r| {
            Ok(Movie {
                id: r.get(0)?,
                title: r.get(1)?,
                approved: r.get(2)?,
                source: r.get(3)?,
                position: r.get(4)?,
                duration: r.get(5)?,
            })
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(rows))
}
/// Runs a filesystem scan for an authenticated Parents request.
async fn scan(State(app): State<App>, headers: HeaderMap) -> ApiResult<Json<usize>> {
    if !app.parent(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(
        tokio::task::spawn_blocking(move || app.scan())
            .await
            .map_err(internal)?
            .map_err(internal)?,
    ))
}
#[derive(Deserialize)]
struct Approval {
    approved: bool,
}

#[derive(Deserialize)]
struct Rename {
    name: String,
}

/// Validates a trimmed display name without accepting control characters.
fn valid_name(name: &str, maximum: usize) -> bool {
    !name.is_empty() && name.chars().count() <= maximum && !name.chars().any(char::is_control)
}

/// Persists a Parents-only display-title change without renaming the media file.
async fn rename_movie(
    State(app): State<App>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<Rename>,
) -> ApiResult<StatusCode> {
    if !app.parent(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let name = input.name.trim();
    if !valid_name(name, 255) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let changed = app
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE movies SET title=?1 WHERE id=?2 AND present=1",
            params![name, id],
        )
        .map_err(internal)?;
    if changed == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
/// Grants or revokes Baby visibility for a present movie.
async fn approve(
    State(app): State<App>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<Approval>,
) -> ApiResult<StatusCode> {
    if !app.parent(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let changed = app
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE movies SET approved=?1 WHERE id=?2 AND present=1",
            params![input.approved, id],
        )
        .map_err(internal)?;
    if changed == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
/// Resolves and streams a visible movie while containing paths within its root.
async fn stream(State(app): State<App>, Path(id): Path<i64>, req: Request) -> ApiResult<Response> {
    let parent = app.parent(req.headers());
    let (source, path): (String, String) = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT source,path FROM movies WHERE id=?1 AND present=1 AND (approved=1 OR ?2)",
            params![id, parent],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let root = app.media.get(&source).ok_or(StatusCode::NOT_FOUND)?;
    let path = tokio::fs::canonicalize(root.join(path))
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !path.starts_with(root) {
        return Err(StatusCode::NOT_FOUND);
    }
    let response = ServeFile::new(path).try_call(req).await.map_err(internal)?;
    Ok(response.map(Body::new))
}
