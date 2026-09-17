use custom_plex::{App, router};
use std::{env, path::PathBuf};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let media = PathBuf::from(env::var("MEDIA_DIR").unwrap_or_else(|_| "media".into()));
    let data = PathBuf::from(env::var("DATA_DIR").unwrap_or_else(|_| "data".into()));
    let sources = match env::var("MEDIA_SOURCES") {
        Ok(value) if !value.trim().is_empty() => serde_json::from_str(&value).map_err(|e| {
            anyhow::anyhow!("MEDIA_SOURCES must be a JSON object mapping names to paths: {e}")
        })?,
        _ => std::collections::BTreeMap::from([("default".to_owned(), media)]),
    };
    let mut app = App::open_sources(
        sources,
        data,
        env::var("COOKIE_SECURE").as_deref() == Ok("true"),
    )?;
    if env::args().nth(1).as_deref() == Some("set-password") {
        let password = if env::args().nth(2).as_deref() == Some("--stdin") {
            let mut value = String::new();
            std::io::stdin().read_line(&mut value)?;
            value.trim_end_matches(['\r', '\n']).to_owned()
        } else {
            let value = rpassword::prompt_password("New parents password (12+ characters): ")?;
            let confirm = rpassword::prompt_password("Confirm password: ")?;
            anyhow::ensure!(value == confirm, "Passwords do not match");
            value
        };
        app.set_password(&password)?;
        println!("Parents password updated; all sessions revoked.");
        return Ok(());
    }
    if let Ok(path) = env::var("PHOTO_DIR") {
        anyhow::ensure!(!path.trim().is_empty(), "PHOTO_DIR must not be empty");
        app = app.with_photo_directory(PathBuf::from(path))?;
    }
    println!("Indexed {} movies", app.scan()?);
    let address = env::var("APP_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    println!("Custom Plex listening on {address}");
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.ok();
    }
}
