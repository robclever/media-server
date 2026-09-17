use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use custom_plex::{App, router};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    bytes: Vec<u8>,
    csrf: bool,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if csrf {
        request = request.header("x-requested-with", "custom-plex");
    }
    app.clone()
        .oneshot(request.body(Body::from(bytes)).unwrap())
        .await
        .unwrap()
}
async fn json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}
fn picture() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(8, 4, image::Rgb([12, 80, 140]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}
fn setup() -> (tempfile::TempDir, Router) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("media")).unwrap();
    let app = App::open(tmp.path().join("media"), tmp.path().join("data"), false)
        .unwrap()
        .with_photo_directory(tmp.path().join("external/photos"))
        .unwrap();
    (tmp, router(app))
}
#[tokio::test]
async fn anonymous_albums_keep_originals_on_external_storage_and_survive_restart() {
    let (tmp, app) = setup();
    let created = send(
        &app,
        "POST",
        "/api/albums",
        br#"{"name":" Summer memories "}"#.to_vec(),
        true,
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(json(created).await["name"], "Summer memories");
    let original = picture();
    let uploaded = send(
        &app,
        "POST",
        "/api/albums/1/photos?name=..%2Fholiday.png",
        original.clone(),
        true,
    )
    .await;
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    let photo = json(uploaded).await;
    let id = photo["id"].as_i64().unwrap();
    for variant in ["thumbnail", "preview", "original"] {
        let response = send(
            &app,
            "GET",
            &format!("/api/photos/{id}/{variant}"),
            vec![],
            false,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        if variant == "original" {
            assert_eq!(bytes.as_ref(), original);
        } else {
            assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 8);
        }
    }
    let folders = std::fs::read_dir(tmp.path().join("external/photos"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(
        std::fs::read(folders[0].path().join("original.png")).unwrap(),
        original
    );
    assert!(!tmp.path().join("external/holiday.png").exists());
    let reopened = router(
        App::open(tmp.path().join("media"), tmp.path().join("data"), false)
            .unwrap()
            .with_photo_directory(tmp.path().join("external/photos"))
            .unwrap(),
    );
    let albums = json(send(&reopened, "GET", "/api/albums", vec![], false).await).await;
    assert_eq!(albums[0]["count"], 1);
    assert_eq!(albums[0]["cover_id"], id);
    assert_eq!(
        json(send(&reopened, "GET", "/api/albums/1/photos", vec![], false).await).await[0]["name"],
        "../holiday.png"
    );
    assert_eq!(
        send(
            &reopened,
            "GET",
            &format!("/api/photos/{id}/original"),
            vec![],
            false
        )
        .await
        .status(),
        StatusCode::OK
    );
}
#[tokio::test]
async fn invalid_uploads_and_unavailable_storage_do_not_create_photos() {
    let (tmp, app) = setup();
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/albums",
            br#"{"name":"Test"}"#.to_vec(),
            false
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/albums",
            br#"{"name":" "}"#.to_vec(),
            true
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    send(
        &app,
        "POST",
        "/api/albums",
        br#"{"name":"Test"}"#.to_vec(),
        true,
    )
    .await;
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/albums/99/photos?name=x.png",
            picture(),
            true
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/albums/1/photos?name=x.png",
            b"not an image".to_vec(),
            true
        )
        .await
        .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/albums/1/photos?name=x.png",
            vec![0; 24 * 1024 * 1024 + 1],
            true
        )
        .await
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        send(&app, "GET", "/api/albums/99/photos", vec![], false)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    std::fs::remove_dir(tmp.path().join("external/photos")).unwrap();
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/albums/1/photos?name=x.png",
            picture(),
            true
        )
        .await
        .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        json(send(&app, "GET", "/api/albums/1/photos", vec![], false).await).await,
        serde_json::json!([])
    );
}
#[cfg(unix)]
#[tokio::test]
async fn photo_symlinks_cannot_escape_storage() {
    let (tmp, app) = setup();
    send(
        &app,
        "POST",
        "/api/albums",
        br#"{"name":"Test"}"#.to_vec(),
        true,
    )
    .await;
    send(
        &app,
        "POST",
        "/api/albums/1/photos?name=x.png",
        picture(),
        true,
    )
    .await;
    let folder = std::fs::read_dir(tmp.path().join("external/photos"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let outside = tmp.path().join("private.png");
    std::fs::write(&outside, b"private").unwrap();
    std::fs::remove_file(folder.join("original.png")).unwrap();
    std::os::unix::fs::symlink(outside, folder.join("original.png")).unwrap();
    assert_eq!(
        send(&app, "GET", "/api/photos/1/original", vec![], false)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
