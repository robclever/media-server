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
pub struct App {
    db: Arc<Mutex<Connection>>,
    media: BTreeMap<String, PathBuf>,
    secure: bool,
    attempts: Arc<Mutex<Vec<i64>>>,
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn internal(_: impl std::fmt::Display) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}
impl App {
    pub fn open(media: PathBuf, data: PathBuf, secure: bool) -> anyhow::Result<Self> {
        Self::open_sources(BTreeMap::from([("default".into(), media)]), data, secure)
    }
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
        let mut db = Connection::open(data.join("library.sqlite3"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE IF NOT EXISTS account (id INTEGER PRIMARY KEY CHECK(id=1), hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sessions (token TEXT PRIMARY KEY, expires INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS movies (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, title TEXT NOT NULL, approved INTEGER NOT NULL DEFAULT 0, present INTEGER NOT NULL DEFAULT 1);")?;
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
            secure,
            attempts: Arc::new(Mutex::new(Vec::new())),
        })
    }
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
fn cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| s.trim().strip_prefix("session="))
}
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
        .route("/health", get(|| async { "ok" }))
        .route("/api/session", get(session))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/movies", get(movies))
        .route("/api/scan", post(scan))
        .route("/api/movies/{id}/approval", post(approve))
        .route("/media/{id}", get(stream))
        .layer(middleware::from_fn(protections))
        .with_state(app)
}
#[derive(Serialize)]
struct Session {
    parent: bool,
}
async fn session(State(app): State<App>, headers: HeaderMap) -> Json<Session> {
    Json(Session {
        parent: app.parent(&headers),
    })
}
#[derive(Deserialize)]
struct Login {
    password: String,
}
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
pub struct Movie {
    pub id: i64,
    pub title: String,
    pub source: String,
    pub approved: bool,
}
async fn movies(State(app): State<App>, headers: HeaderMap) -> ApiResult<Json<Vec<Movie>>> {
    let parent = app.parent(&headers);
    let db = app.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT id,title,approved,source FROM movies WHERE present=1 AND (approved=1 OR ?1) ORDER BY title COLLATE NOCASE").map_err(internal)?;
    let rows = stmt
        .query_map([parent], |r| {
            Ok(Movie {
                id: r.get(0)?,
                title: r.get(1)?,
                approved: r.get(2)?,
                source: r.get(3)?,
            })
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(rows))
}
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
