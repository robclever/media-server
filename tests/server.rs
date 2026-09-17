use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use custom_plex::{App, Movie, router};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: &str,
    body: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .header("x-requested-with", "custom-plex")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn list(app: &Router, cookie: &str) -> Vec<Movie> {
    let response = request(app, "GET", "/api/movies", cookie, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}
async fn login(app: &Router) -> String {
    let response = request(
        app,
        "POST",
        "/api/login",
        "",
        r#"{"password":"long-test-password"}"#,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    cookie.split(';').next().unwrap().to_owned()
}
fn setup() -> (tempfile::TempDir, App) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("media")).unwrap();
    std::fs::write(tmp.path().join("media/Family.mp4"), b"0123456789").unwrap();
    std::fs::write(tmp.path().join("media/Private.mp4"), b"private").unwrap();
    let state = App::open(tmp.path().join("media"), tmp.path().join("data"), false).unwrap();
    state.set_password("long-test-password").unwrap();
    assert_eq!(state.scan().unwrap(), 2);
    (tmp, state)
}
#[tokio::test]
async fn authorization_streaming_and_persistence() {
    let (tmp, state) = setup();
    let app = router(state.clone());
    assert!(list(&app, "").await.is_empty());
    assert_eq!(
        request(&app, "POST", "/api/scan", "", "{}").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "POST", "/api/login", "", r#"{"password":"wrong"}"#)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = login(&app).await;
    let movies = list(&app, &cookie).await;
    assert_eq!(movies.len(), 2);
    let id = movies.iter().find(|m| m.title == "Family").unwrap().id;
    let media = format!("/media/{id}");
    let approval = format!("/api/movies/{id}/approval");
    assert_eq!(
        request(&app, "GET", &media, "", "").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "POST", &approval, "", r#"{"approved":true}"#)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "POST", &approval, &cookie, r#"{"approved":true}"#)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(list(&app, "").await.len(), 1);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&media)
                .header("range", "bytes=2-5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["content-range"], "bytes 2-5/10");
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        "2345"
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&media)
                .header("range", "bytes=100-200")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        request(&app, "HEAD", &media, "", "").await.status(),
        StatusCode::OK
    );
    let reopened =
        router(App::open(tmp.path().join("media"), tmp.path().join("data"), false).unwrap());
    assert_eq!(list(&reopened, "").await.len(), 1);
    assert_eq!(
        request(&app, "POST", "/api/logout", &cookie, "{}")
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(list(&app, &cookie).await.len(), 1);
    assert_eq!(
        request(&app, "POST", "/api/scan", &cookie, "{}")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = login(&app).await;
    assert_eq!(
        request(&app, "POST", &approval, &cookie, r#"{"approved":false}"#)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(&app, "GET", &media, "", "").await.status(),
        StatusCode::NOT_FOUND
    );
    state.set_password("replacement-password").unwrap();
    assert_eq!(
        request(&app, "POST", "/api/scan", &cookie, "{}")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
#[tokio::test]
async fn session_expiry_csrf_and_scan_removal() {
    let (tmp, state) = setup();
    let app = router(state.clone());
    let cookie = login(&app).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/scan")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let db = rusqlite::Connection::open(tmp.path().join("data/library.sqlite3")).unwrap();
    db.execute("UPDATE sessions SET expires=0", []).unwrap();
    assert_eq!(
        request(&app, "POST", "/api/scan", &cookie, "{}")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    std::fs::remove_file(tmp.path().join("media/Private.mp4")).unwrap();
    state.scan().unwrap();
    let cookie = login(&app).await;
    assert_eq!(list(&app, &cookie).await.len(), 1);
    for _ in 0..10 {
        request(&app, "POST", "/api/login", "", r#"{"password":"wrong"}"#).await;
    }
    assert_eq!(
        request(&app, "POST", "/api/login", "", r#"{"password":"wrong"}"#)
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}
#[cfg(unix)]
#[tokio::test]
async fn symlinks_cannot_escape_library() {
    let (tmp, state) = setup();
    std::fs::write(tmp.path().join("secret.mp4"), b"secret").unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("secret.mp4"),
        tmp.path().join("media/link.mp4"),
    )
    .unwrap();
    assert_eq!(state.scan().unwrap(), 2);
    let app = router(state);
    let cookie = login(&app).await;
    let id = list(&app, &cookie)
        .await
        .iter()
        .find(|m| m.title == "Family")
        .unwrap()
        .id;
    std::fs::remove_file(tmp.path().join("media/Family.mp4")).unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("secret.mp4"),
        tmp.path().join("media/Family.mp4"),
    )
    .unwrap();
    assert_eq!(
        request(&app, "GET", &format!("/media/{id}"), &cookie, "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn multiple_sources_keep_files_and_approvals_separate() {
    use std::collections::BTreeMap;
    let (tmp, original) = setup();
    let old_app = router(original);
    let cookie = login(&old_app).await;
    let old_id = list(&old_app, &cookie)
        .await
        .into_iter()
        .find(|m| m.title == "Family")
        .unwrap()
        .id;
    request(
        &old_app,
        "POST",
        &format!("/api/movies/{old_id}/approval"),
        &cookie,
        r#"{"approved":true}"#,
    )
    .await;
    let extra = tmp.path().join("extra");
    std::fs::create_dir(&extra).unwrap();
    std::fs::write(extra.join("Family.mp4"), b"second-drive").unwrap();
    let roots = BTreeMap::from([
        ("default".into(), tmp.path().join("media")),
        ("archive".into(), extra.clone()),
    ]);
    let state = App::open_sources(roots.clone(), tmp.path().join("data"), false).unwrap();
    assert_eq!(state.scan().unwrap(), 3);
    let app = router(state.clone());
    let all = list(&app, &cookie).await;
    let second = all.iter().find(|m| m.source == "archive").unwrap();
    assert_ne!(second.id, old_id);
    assert!(!second.approved);
    assert_eq!(list(&app, "").await[0].id, old_id);
    let url = format!("/media/{}", second.id);
    assert_eq!(
        request(&app, "GET", &url, "", "").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "GET", &url, &cookie, "")
            .await
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
        "second-drive"
    );
    // A failed scan must not publish a partial library or lose approvals.
    std::fs::rename(&extra, tmp.path().join("offline")).unwrap();
    assert!(state.scan().is_err());
    assert_eq!(list(&app, "").await[0].id, old_id);
    std::fs::rename(tmp.path().join("offline"), &extra).unwrap();
    state.scan().unwrap();
    // Source removal blocks direct access as well as listings, even before a rescan.
    let reduced = App::open_sources(
        BTreeMap::from([("default".into(), tmp.path().join("media"))]),
        tmp.path().join("data"),
        false,
    )
    .unwrap();
    let reduced_app = router(reduced);
    assert_eq!(list(&reduced_app, &cookie).await.len(), 2);
    assert_eq!(
        request(&reduced_app, "GET", &url, &cookie, "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let restored = App::open_sources(roots, tmp.path().join("data"), false).unwrap();
    restored.scan().unwrap();
    assert_eq!(
        list(&router(restored), &cookie)
            .await
            .iter()
            .find(|m| m.source == "archive")
            .unwrap()
            .id,
        second.id
    );
}

#[tokio::test]
async fn legacy_database_migrates_without_losing_ids_or_approvals() {
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media");
    let data = tmp.path().join("data");
    std::fs::create_dir(&media).unwrap();
    std::fs::create_dir(&data).unwrap();
    std::fs::write(media.join("Old.mp4"), b"legacy-video").unwrap();
    let db = rusqlite::Connection::open(data.join("library.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE movies (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, title TEXT NOT NULL, approved INTEGER NOT NULL DEFAULT 0, present INTEGER NOT NULL DEFAULT 1); INSERT INTO movies VALUES(42,'Old.mp4','Old',1,1);").unwrap();
    drop(db);
    let state = App::open(media.clone(), data.clone(), false).unwrap();
    state.scan().unwrap();
    let app = router(state);
    let movies = list(&app, "").await;
    assert_eq!(movies.len(), 1);
    assert_eq!(movies[0].id, 42);
    assert_eq!(movies[0].source, "default");
    assert!(movies[0].approved);
    assert_eq!(
        request(&app, "GET", "/media/42", "", "").await.status(),
        StatusCode::OK
    );
    let again = App::open(media, data, false).unwrap();
    again.scan().unwrap();
    assert_eq!(list(&router(again), "").await[0].id, 42);
}

#[test]
fn invalid_source_configuration_is_rejected() {
    use std::collections::BTreeMap;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("media");
    std::fs::create_dir_all(root.join("nested")).unwrap();
    for sources in [
        BTreeMap::new(),
        BTreeMap::from([("bad name".into(), root.clone())]),
        BTreeMap::from([("missing".into(), tmp.path().join("missing"))]),
        BTreeMap::from([("a".into(), root.clone()), ("b".into(), root.clone())]),
        BTreeMap::from([
            ("a".into(), root.clone()),
            ("b".into(), root.join("nested")),
        ]),
    ] {
        assert!(App::open_sources(sources, tmp.path().join("data"), false).is_err());
    }
}

fn cover_image() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(16, 16, image::Rgb([220, 120, 40]));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes)
        .encode_image(&image)
        .unwrap();
    bytes
}
async fn upload_cover(
    app: &Router,
    id: i64,
    cookie: &str,
    bytes: Vec<u8>,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/movies/{id}/cover"))
                .header("cookie", cookie)
                .header("x-requested-with", "custom-plex")
                .header("content-type", "image/jpeg")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap()
}
#[tokio::test]
async fn visible_movie_covers_can_be_changed_without_parent_login() {
    let (tmp, state) = setup();
    let app = router(state);
    let cookie = login(&app).await;
    let id = list(&app, &cookie).await[0].id;
    let cover = format!("/api/movies/{id}/cover");
    assert_eq!(
        upload_cover(&app, id, "", cover_image()).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        upload_cover(&app, id, &cookie, cover_image())
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(&app, "GET", &cover, "", "").await.status(),
        StatusCode::NOT_FOUND
    );
    request(
        &app,
        "POST",
        &format!("/api/movies/{id}/approval"),
        &cookie,
        r#"{"approved":true}"#,
    )
    .await;
    assert_eq!(
        upload_cover(&app, id, "", cover_image()).await.status(),
        StatusCode::NO_CONTENT
    );
    let response = request(&app, "GET", &cover, "", "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/jpeg");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(image::load_from_memory(&body).is_ok());
    assert_eq!(
        upload_cover(&app, id, "", b"not an image".to_vec())
            .await
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        upload_cover(&app, id, "", vec![0; 8 * 1024 * 1024 + 1])
            .await
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        request(&app, "GET", &cover, "", "").await.status(),
        StatusCode::OK
    );
    let reopened =
        router(App::open(tmp.path().join("media"), tmp.path().join("data"), false).unwrap());
    assert_eq!(
        request(&reopened, "GET", &cover, "", "").await.status(),
        StatusCode::OK
    );
    request(
        &app,
        "POST",
        &format!("/api/movies/{id}/approval"),
        &cookie,
        r#"{"approved":false}"#,
    )
    .await;
    assert_eq!(
        request(&app, "GET", &cover, "", "").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "DELETE", &cover, "", "").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "DELETE", &cover, &cookie, "").await.status(),
        StatusCode::NO_CONTENT
    );
}
#[tokio::test]
async fn sidecar_covers_work_and_cannot_escape_the_media_root() {
    let (tmp, state) = setup();
    let app = router(state);
    let cookie = login(&app).await;
    let id = list(&app, &cookie)
        .await
        .into_iter()
        .find(|m| m.title == "Family")
        .unwrap()
        .id;
    let cover = format!("/api/movies/{id}/cover");
    std::fs::write(tmp.path().join("media/Family.jpg"), cover_image()).unwrap();
    assert_eq!(
        request(&app, "GET", &cover, &cookie, "").await.status(),
        StatusCode::OK
    );
    // An uploaded image wins even after the sidecar changes or becomes invalid.
    upload_cover(&app, id, &cookie, cover_image()).await;
    std::fs::write(tmp.path().join("media/Family.jpg"), b"broken sidecar").unwrap();
    assert_eq!(
        request(&app, "GET", &cover, &cookie, "").await.status(),
        StatusCode::OK
    );
    request(&app, "DELETE", &cover, &cookie, "").await;
    #[cfg(unix)]
    {
        std::fs::remove_file(tmp.path().join("media/Family.jpg")).unwrap();
        std::fs::write(tmp.path().join("private.jpg"), cover_image()).unwrap();
        std::os::unix::fs::symlink(
            tmp.path().join("private.jpg"),
            tmp.path().join("media/Family.jpg"),
        )
        .unwrap();
        // The movie fixture is deliberately not a decodable video: no fallback exists.
        assert_eq!(
            request(&app, "GET", &cover, &cookie, "").await.status(),
            StatusCode::NOT_FOUND
        );
    }
}
