# Custom Plex · Family Cinema

A Rust media server for a home DVD library, with a TV-friendly web interface, anonymous **Baby** access, and password-protected **Parents** access. Run it locally or deploy the Docker image on a Raspberry Pi with a 64-bit OS.

This is a standalone Plex-like application. It does not use official Plex clients or Plex accounts.

## Start here for your first test

- **Try it on this Mac:** follow [Tomorrow's local test](#tomorrows-local-test).
- **Put it on the Pi now, without GitHub:** follow [Deploy from source](#deploy-from-source-no-published-image-needed), after preparing Docker on the Pi.
- **Add your movies:** follow [Add movies step by step](#add-movies-step-by-step).
- **Deploy a future published release:** use the numbered [Raspberry Pi deployment instructions](#deploy-on-a-raspberry-pi).

Run commands on the machine named above each example. Replace `PI_USER` and `PI_ADDRESS` with your Pi's login name and IP address; these are placeholders, not literal credentials. `localhost` always means the device on which the browser or command is running.

### Tomorrow's local test

Start Docker Desktop. In a terminal on this Mac:

```sh
cd /Users/robclever/Documents/programming/custom_plex
docker compose up -d --no-build --pull never --wait
docker compose ps
```

The image was already built during implementation. If Docker reports that `custom-plex:local` is missing, run `docker compose build` and repeat the startup command. Do not overwrite your existing `.env` when resuming.

1. If you have not chosen the parents password yet, run `docker compose exec app custom-plex set-password`. Setting it again resets the password and signs out existing parents sessions.
2. Open [localhost:8080](http://localhost:8080), choose **Parents**, and sign in.
3. Click **Scan library**. The generated **Playback-Test** sample should appear.
4. Play it and try seeking. It is an eight-second test pattern with a tone, not a DVD movie.
5. Click **Add to Baby**. The button should read **Available to Baby**.
6. Click **Switch profile**, choose **Baby**, and play the sample without entering a password.
7. Switch back to **Parents** and confirm a password is required again.
8. Add one of your own prepared movies using the instructions below, then test it before copying your entire library.

For a TV on the same home network, use `http://<this-macs-lan-ip>:8080`, not `localhost`. Keep Docker Desktop running and the Mac awake. On the Pi, use its LAN IP instead. If the TV cannot connect, test the same address from another device and check host firewall/network isolation settings.

## What works

- Profile chooser, searchable movie shelf, keyboard/remote navigation, and browser video playback.
- Baby sees and streams only titles explicitly approved by a parent.
- Parents can browse everything, scan the library, and change Baby approvals.
- Argon2 password hashing, eight-hour sessions, logout, password reset, and login throttling.
- Recursive library scans on startup and on demand; approvals and accounts persist in SQLite.
- Byte-range video streaming for seeking, without loading whole movies into memory.
- Docker Compose with persistent data, read-only media, health checks, and automatic restarts.
- GitHub Actions configuration for Rust checks, native AMD64/ARM64 container tests, and versioned image publishing.

No DVD ripping, live transcoding, online poster lookup, external subtitles, native TV app, or internet-facing deployment is included. Titles come from filenames; covers use uploaded images, matching local artwork, or automatically extracted movie frames.

## Quick start: local Rust development

Install [Rust through rustup](https://rustup.rs/). The repository pins Rust 1.96.0 and includes `Cargo.lock`. Rustup installs the pinned toolchain when you first run Cargo. A native C compiler is required for bundled SQLite; on macOS use Xcode Command Line Tools, or on Debian install `build-essential`.

From this project's directory:

```sh
cp .env.example .env
mkdir -p media data
# Copy your prepared video files into media/.
cargo run --locked -- set-password
cargo run --locked
```

The password command prompts twice, without displaying the password. Use at least 12 characters. There is no default password and no browser-based account bootstrap. Run the same command to reset a forgotten password; it invalidates every parent session.

Open [localhost:8080](http://localhost:8080). Choose **Parents**, enter your password, and use **Add to Baby** on appropriate titles. **Switch profile** signs out; choosing Baby also clears the parent session in that browser.

`.env` is loaded automatically. Stop the local server with Ctrl+C. If Docker is already using port 8080, stop it or run `APP_BIND=127.0.0.1:8081 cargo run --locked` and open port 8081.

## Quick start: local Docker

Install and start Docker Desktop, or Docker Engine with the Compose plugin. Create `.env` from `.env.example` if you have not already done so, then:

```sh
mkdir -p media data
```

**Linux hosts:** before starting the container, give its UID/GID access to the data directory:

```sh
sudo chown -R 10001:10001 data
sudo chmod 700 data
```

Docker Desktop on macOS manages bind-mount ownership differently; the above ownership change is generally unnecessary there. If you later switch between native Rust and Docker on Linux, use separate data directories or adjust ownership for the process that will use the directory.

Then build, start, and choose the password:

```sh
docker compose build
docker compose up -d --no-build --pull never --wait
docker compose exec app custom-plex set-password
```

Open [localhost:8080](http://localhost:8080). The container runs as UID/GID `10001:10001`, with a read-only root filesystem, no Linux capabilities, and no-new-privileges enabled.

Useful commands:

```sh
docker compose ps
docker compose logs --tail=100 app
curl --fail http://localhost:8080/health
docker compose stop app
docker compose up -d --no-build --pull never --wait
docker compose down
```

Stopping or removing the container preserves the host's `data/` and `media/` directories. After changing Rust or web source files, rebuild the image and run the startup command again. Web assets are embedded in the binary.

## Prepare your media

The server reads video files, not physical DVD discs, ISO images, or DVD folder structures. Prepare your DVDs as individual files before copying them into the library.

MP4 with H.264 video and AAC audio is the initial playback target. Verify a sample on your actual TV before converting a large library. The scanner includes `.mp4`, `.m4v`, `.webm`, `.mov`, and `.mkv` files, but listing a file does not guarantee the browser supports its codecs or container. Playback errors appear in the video dialog.

Example conversion, if FFmpeg is installed:

```sh
ffmpeg -i input.mkv -c:v libx264 -crf 20 -preset medium -pix_fmt yuv420p \
  -c:a aac -b:a 192k -movflags +faststart output.mp4
```

This prepares a playable file ahead of time; the server does not transcode during playback. Audio track selection and subtitle preparation are up to your conversion workflow.

The scanner ignores symlinks, preserves approvals by location name and relative file path, and hides removed files after a scan. New paths are unapproved by default. Replacing content at an already approved path retains its approval, so review that approval when replacing files. Only trusted household administrators should have write access to the media directory.

## Add movies step by step

There is **no web upload button**. Copy files into the host media folder, then ask the server to scan it. You do not copy movies into the Docker image, and adding a movie does not require rebuilding or restarting the container.

### 1. Find the correct media folder

- **This Mac, default setup:** `/Users/robclever/Documents/programming/custom_plex/media`.
- **Docker on the Pi:** the directory named by `HOST_MEDIA_DIR` in the Pi's `.env`, such as `/mnt/dvd-library`.
- **Native Rust:** the directory named by `MEDIA_DIR`.

`/media` is the path *inside the container*. It maps to the host folder above. `data/` holds the database and is not a place to put movies.

Subfolders are scanned recursively. Give files descriptive, distinct names because the interface derives titles from filenames:

```text
media/
  Finding Nemo (2003).mp4
  Family Favorites/
    Toy Story (1995).mp4
  Parents Movies/
    Example Movie (2020).mp4
```

Folder names do not grant or restrict access. Every newly discovered file needs its own Baby approval, regardless of its folder. Different files with the same filename can produce identical display titles.

### 2. Copy a complete, prepared video

**On this Mac**, use Finder to copy into `media/`, or use Terminal. Replace the source path with your actual video:

```sh
cd /Users/robclever/Documents/programming/custom_plex
cp "/path/to/My Movie.mp4" "media/My Movie.mp4.uploading"
mv "media/My Movie.mp4.uploading" "media/My Movie.mp4"
```

The temporary `.uploading` suffix keeps an incomplete copy out of library scans. Rename it to the supported extension after copying finishes. Choose a new destination name to avoid replacing an existing movie accidentally.

**From your computer to the Pi**, enable SSH on the Pi and use its normal login account. For a new movie, copy into that account's home directory first:

```sh
scp "/path/to/My Movie.mp4" PI_USER@PI_ADDRESS:incoming-movie.mp4
ssh PI_USER@PI_ADDRESS
```

Then **on the Pi**, with `/mnt/dvd-library` already mounted and chosen as `HOST_MEDIA_DIR`:

```sh
sudo install -m 644 "$HOME/incoming-movie.mp4" "/mnt/dvd-library/My Movie.mp4.uploading"
sudo mv "/mnt/dvd-library/My Movie.mp4.uploading" "/mnt/dvd-library/My Movie.mp4"
```

This leaves the staging copy in your Pi home directory; remove it yourself once playback is verified if you want the disk space back. For a library inside your Pi home directory, use that configured path in place of `/mnt/dvd-library` and omit `sudo` where your account already owns the destination.

On Linux, the container's UID 10001 needs read permission on files and execute/traverse permission on each media directory. Mode `644` on a new movie allows reading. For a dedicated newly created library folder, mode `755` allows traversal; existing drive permissions or FAT/exFAT mount options may need a different setup. Do not recursively change permissions on an unrelated shared drive. With the server running, check the mount from inside it:

```sh
docker compose exec app ls -l /media
```

### 3. Scan and verify in Parents

1. Open the server address and sign in as **Parents**.
2. Click **Scan library** and wait for the library-updated message.
3. Search for the filename's title and open it.
4. Confirm video, sound, seeking, and fullscreen work on the TV.

Scans happen on startup and when you click **Scan library**. There is no automatic file watcher. The displayed scan count is the total number of supported media files found, not just newly added files.

### 4. Decide whether Baby can watch it

Click **Add to Baby** for an approved movie, then switch to Baby to check that it appears. Leave the button unchanged for parents-only content. Click **Available to Baby** while signed in as Parents to revoke approval. Reopen the Baby library or refresh another device to update its displayed list.

### Rename, replace, or remove movies

- **Rename/move:** change the file on the host, then scan. A different relative path is a new entry and needs approval again, unless that exact path existed previously with a stored approval.
- **Replace at the same path:** the existing approval remains. Revoke Baby approval before replacing content if it should be reviewed again.
- **Remove:** move the file outside the media folder or delete it, then scan. Its card disappears. Stored database entries are retained, so reintroducing the exact same path restores its previous approval.
- **Missing after a scan:** check that the copy completed, its final extension is supported, it is inside the configured folder, it is not a symlink, and the container can read it. Then check whether you are viewing Baby or Parents.

## Movie cover images

Every movie card has **Add / change image** and **Use automatic image** buttons. Both Baby and Parents may change the image of a movie they can see. Baby cannot view or change images for parents-only movies. Cover changes are shared across devices and do not change a movie's Baby approval.

### Upload an image

1. Open the catalog and find the movie.
2. Click **Add / change image** and choose a JPEG, PNG, or WebP from your computer or phone.
3. Wait for the image-saved message. The new image replaces the catalog placeholder or automatic cover.

The maximum upload is 8 MiB, with at most 8192 pixels on either side and a bounded decoding memory budget. Large images that exceed the decoding budget are also rejected. SVG, GIF, and HEIC are not supported; export a JPEG or PNG first. Images are decoded, resized to fit within 640 × 640 pixels, and stored as JPEGs, rather than served as arbitrary uploaded files. The original media directory remains read-only.

### Automatic image selection

When no uploaded image exists, the server checks, in order:

1. An image with the **same filename stem beside the video**, checking `.jpg`, `.jpeg`, `.png`, then `.webp` (lowercase extensions). For example, `Finding Nemo (2003).mp4` can use `Finding Nemo (2003).jpg` in the same directory. This works in each configured storage location; matching names on different drives remain independent.
2. A video frame at approximately 10 seconds, or the beginning if the video is too short.
3. The existing play-symbol placeholder if neither artwork nor a decodable frame is available.

Images are generated on demand as cards enter view, so the first load may take a little time. Work is serialized to limit load on the Pi, with a 20-second timeout per extraction attempt. Results are cached in SQLite. If the video or matching image changes its size or modification timestamp, its automatic cover is refreshed on the next request. Unusable-image/frame results can be retried with **Use automatic image**.

**Use automatic image** deletes the saved cover/cache for that movie and repeats the order above. It does not remove a matching image from the media folder; remove or rename that image yourself if you specifically want a movie frame instead. It never deletes the video. To see another device's image change, refresh the catalog.

FFmpeg is included in the Docker image. For native Rust development, install FFmpeg separately and ensure `ffmpeg` is on the server process's `PATH`; uploads and matching images still work without it, but frame extraction cannot. This feature does not perform playback transcoding or download posters from external services.

Uploaded and generated images live in the `covers` table in `DATA_DIR/library.sqlite3`, so existing data backups include them. Cover records use the movie ID and survive rescans and container recreation. Deleted/removed library entries retain their cached cover if later restored, just as they retain approvals. The schema change is additive.

For an existing Docker deployment, rebuild/pull an image with this feature and recreate the container. Local build:

```sh
docker compose build
docker compose up -d --no-build --pull never --wait
```

## Configure additional storage locations

Set **`MEDIA_SOURCES`** in `.env` to a JSON object mapping stable names to directories. All configured locations are scanned recursively into one library. Parents see a **Location** label on each card to distinguish identical movie titles.

Leave `MEDIA_SOURCES` blank or unset to keep the existing single-folder `MEDIA_DIR` setup. A nonempty value **replaces** `MEDIA_DIR`; include your original folder under the name **`default`** to preserve its existing movie IDs and Baby approvals.

### Native Rust: multiple folders on this computer

Edit `.env` (use paths that actually exist on the machine running Rust):

```dotenv
MEDIA_SOURCES='{"default":"./media","archive":"/Volumes/Archive/Movies","family":"/Volumes/Family Drive/Videos"}'
```

On a Pi running Rust directly, use Linux paths such as `/mnt/archive/Movies` instead of `/Volumes/...`. Spaces in paths work inside the JSON strings. Stop and restart `cargo run --locked` after changing configuration. Startup scans every location; later file additions need **Scan library** in Parents.

### Docker: add a second drive or folder

Docker needs both a **host mount** and an **application location**. Setting a host path in `MEDIA_SOURCES` alone does not make it available inside the container.

1. Mount the drive or network share on the Docker host and confirm its movie folder exists.
2. In the project/deployment directory, copy the supplied example:

   ```sh
   cp compose.storage.example.yaml compose.override.yaml
   ```

   If you already have `compose.override.yaml`, merge the example's volume entry into it instead of overwriting your existing settings. Compose loads this override automatically alongside `compose.yaml`.

3. Edit `.env`, keeping the existing `HOST_MEDIA_DIR` and adding:

   ```dotenv
   HOST_ARCHIVE_DIR=/mnt/archive/Movies
   MEDIA_SOURCES='{"default":"/media","archive":"/media-archive"}'
   ```

   For Docker Desktop on this Mac, `HOST_ARCHIVE_DIR` might be `/Volumes/Archive/Movies` or another existing local folder. The JSON still uses `/media-archive`, because that is the path inside Docker. Docker Desktop must have access to the host folder.

4. Apply and verify:

   ```sh
   docker compose config --quiet
   docker compose up -d --no-build --pull never --wait
   docker compose logs --tail=100 app
   docker compose exec app ls -l /media-archive
   ```

   Use an image containing this feature: rebuild with `docker compose build` first if you still have an older local image. `docker compose restart` alone does not apply changed environment variables or mounts.

5. Sign in as Parents. Both locations appear together; newly discovered movies remain parents-only until you approve them. Use **Scan library** after subsequent file copies.

The example mount is read-only and sets `create_host_path: false`, so a mistyped or missing host folder fails deployment instead of being silently created. This does not detect a disconnected drive whose empty mount-point directory still exists; verify that external storage is mounted before starting/scanning.

### Add a third location or a mounted network share

Mount SMB/NFS storage on the host OS first; this app accepts filesystem paths, not `smb://` URLs or network-share credentials. Mount configuration and credentials remain with the host OS.

Add another item under `services.app.volumes` in your override, alongside the archive entry:

```yaml
      - type: bind
        source: /mnt/family-share/Movies
        target: /media-family
        read_only: true
        bind:
          create_host_path: false
```

Then extend the same `.env` value:

```dotenv
MEDIA_SOURCES='{"default":"/media","archive":"/media-archive","family":"/media-family"}'
```

Run the apply/verify commands again. Repeat this pattern for more locations, with a unique source name and container target for each. Keep the SQLite `DATA_DIR` on a local writable filesystem even when movies live on a network share. For a published-image deployment, copy `compose.storage.example.yaml` to the Pi as well, or create the override using the example above.

### Location names, availability, and existing approvals

- Names may contain letters, digits, underscores, and hyphens. Keep them stable: a movie is identified by **location name + relative file path**, not list order. Identical filenames on different locations have independent approvals.
- Keep `default` attached to the original library when upgrading. The database migration preserves its movie IDs and approvals automatically. Back up the data directory before the first upgrade; an older binary cannot use the new multi-location schema, so rollback requires the matching pre-upgrade backup.
- Renaming a location creates separate entries that start unapproved. Removing a location hides its movies and blocks streaming; it does not delete video files or their saved approvals. Re-adding the same name and relative paths restores those approvals.
- Changing the folder behind an existing name retains approvals for matching relative paths. Use a new name for unrelated content, or review approvals before switching the folder.
- Every configured folder must exist and be readable at startup. Duplicate or nested/overlapping roots are rejected to prevent double indexing. A scan that encounters an unreadable/missing folder fails without committing a partial library; restore the folder or remove its configuration and recreate the container. Check logs for startup errors. A previously listed unavailable movie cannot play until its file returns.
- No location-management web form or automatic drive discovery is provided. Configuration changes require a server restart/container recreation; ordinary new movies only require a scan.

## Configuration

| Setting | Default | Purpose |
| --- | --- | --- |
| `APP_BIND` | `127.0.0.1:8080` | Native server listening address; Compose sets `0.0.0.0:8080` inside the container |
| `MEDIA_DIR` | `./media` | Single media directory when `MEDIA_SOURCES` is blank; Compose uses `/media` |
| `MEDIA_SOURCES` | blank | JSON object of location names to server-visible paths; overrides `MEDIA_DIR` |
| `DATA_DIR` | `./data` | Native database directory; Compose uses `/data` |
| `COOKIE_SECURE` | `false` | Set `true` only when serving through HTTPS; secure cookies do not work over ordinary LAN HTTP |
| `HOST_MEDIA_DIR` | `./media` | Compose host directory mounted read-only at `/media` |
| `HOST_DATA_DIR` | `./data` | Compose host directory mounted at `/data` |
| `PORT` | `8080` | Compose published host port |
| `PLEX_IMAGE` | `custom-plex:local` | Locally built image or published version/digest |

Use a local filesystem for the database, not a network share. SQLite uses a rollback journal and transactions. Keep the media drive mounted at a stable path before starting the service.

## Deploy on a Raspberry Pi

### 1. Prepare the Pi

Use a Pi capable of running a **64-bit Linux OS**. Verify `uname -m` reports `aarch64`; 32-bit images are not built. Actual Pi performance and a minimum hardware specification still need hardware testing.

For 64-bit Raspberry Pi OS, follow the official [Docker Engine installation instructions for Debian](https://docs.docker.com/engine/install/debian/), including the Compose plugin. Docker directs 64-bit Pi installations to Debian ARM64 packages. Check that `docker version` and `docker compose version` work. If Docker requires elevated access on your installation, use `sudo` consistently for Docker commands.

Use reliable power, preferably wired networking, and a USB drive for a large library. Mount that drive before starting the application; configure the OS to mount it at boot. Use the Pi's local disk for the database.

### Deploy from source (no published image needed)

This is the usable deployment path now, before a GitHub release image exists. Complete **Prepare the Pi** above first. No Rust installation is needed on the Pi: Docker builds the Rust binary.

**On this Mac**, package only the source/build files and send them to the Pi. This excludes your local password database, `.env`, videos, and build output:

```sh
cd /Users/robclever/Documents/programming/custom_plex
tar -czf /tmp/custom-plex-source.tar.gz \
  Cargo.toml Cargo.lock rust-toolchain.toml Dockerfile .dockerignore \
  compose.yaml compose.storage.example.yaml .env.example README.md src web scripts tests .github
scp /tmp/custom-plex-source.tar.gz PI_USER@PI_ADDRESS:custom-plex-source.tar.gz
ssh PI_USER@PI_ADDRESS
```

**On the Pi**, for a first deployment into a new directory:

```sh
mkdir -p "$HOME/custom-plex"
tar -xzf "$HOME/custom-plex-source.tar.gz" -C "$HOME/custom-plex"
cd "$HOME/custom-plex"
cp .env.example .env
mkdir -p media data
chmod 755 media
sudo chown 10001:10001 data
sudo chmod 700 data
docker compose build
docker compose up -d --no-build --pull never --wait
docker compose exec app custom-plex set-password
docker compose ps
curl --fail http://localhost:8080/health
hostname -I
```

Keep the default `PLEX_IMAGE=custom-plex:local`. This first deployment stores movies in `$HOME/custom-plex/media` and the database in `$HOME/custom-plex/data`. To use a USB drive, mount it first and edit `HOST_MEDIA_DIR` in `.env` to its absolute library path before startup. After changing a mount setting, run the startup command again to recreate the container with the new mount. Switching media roots while keeping the database preserves approvals for matching relative paths; review them when changing libraries.

The health command should print `ok`; Compose should report the container as healthy. Use the Pi's home-network address from `hostname -I` (not a Docker bridge address) to open `http://<pi-ip>:8080` on your TV. Follow **Add movies step by step**, using your chosen host library path. Continue with **Open it on the TV** below; the published-image steps are an alternative, not additional steps required for this source deployment.

For later source updates, copy the updated source files without overwriting `.env`, `data/`, or `media/`, back up the database, and run `docker compose build` followed by the startup command. No registry pull is needed for `custom-plex:local`.

### 2. Install the deployment files (published-image alternative)

For this alternative, first create `/srv/custom-plex/` on the Pi (`sudo mkdir -p /srv/custom-plex`). Copy `compose.yaml` and `.env.example` from the release you intend to deploy there, and ensure your deployment account can edit its configuration. Then:

```sh
cd /srv/custom-plex
cp .env.example .env
sudo install -d -o 10001 -g 10001 -m 700 /srv/custom-plex/data
```

Edit `.env` with your actual paths and the published version:

```dotenv
HOST_MEDIA_DIR=/mnt/dvd-library
HOST_DATA_DIR=/srv/custom-plex/data
PORT=8080
PLEX_IMAGE=ghcr.io/YOUR_OWNER/YOUR_REPOSITORY:v0.1.0
COOKIE_SECURE=false
```

Replace the owner/repository/version placeholders with a real published image. No image has been published by this local implementation. The media directory must exist, and UID 10001 must be able to traverse its folders and read its files. Do not change ownership of your entire media drive without considering its other uses.

### 3. Start and set the parents password

For a private GHCR package, first run `docker login ghcr.io` using an account/token allowed to read the package. Then:

```sh
cd /srv/custom-plex
docker compose pull
docker compose up -d --no-build --pull never --wait
docker compose exec app custom-plex set-password
docker compose ps
curl --fail http://localhost:8080/health
```

The same interactive command resets the password later. For an automated, isolated test, `custom-plex set-password --stdin` accepts one password line on standard input; never commit household passwords or put them in command-line arguments.

**Before an image is published:** copy the full repository to the Pi, keep `PLEX_IMAGE=custom-plex:local`, and run `docker compose build` before startup. This compiles on the Pi and can take longer than pulling an image.

### 4. Open it on the TV

Visit `http://<pi-ip-address>:8080`. `http://raspberrypi.local:8080` also works when the hostname and network discovery are supported by the TV. Reserve the Pi's IP in your router for a stable address.

The TV needs a web browser with HTML5 video support. Arrow keys move between interface controls; Enter activates them. Inputs and the native video player keep their own key handling. A TV without a suitable browser needs a separate browser-capable device.

Sign in as Parents, scan the library, and approve titles. Then switch to Baby and verify playback, seeking, fullscreen, remote navigation, and audio on the actual hardware. The deployment restarts automatically after a Pi reboot when Docker and the storage mounts are available.

### Network and access boundaries

This configuration is for a trusted home network. Anyone who can reach the server can use Baby. Parent authorization is checked on both library and video requests; guessing a hidden movie ID does not grant access. Sessions expire after eight hours and logout revokes them.

Login attempts are limited to ten per minute across the server. State-changing browser requests require a custom header, cookies use `HttpOnly` and `SameSite=Strict`, and responses disable caching. Existing video bytes already delivered to a browser cannot be recalled by revoking an approval.

HTTP does not encrypt credentials. Do not forward port 8080 to the internet. Remote access and HTTPS need a separate reverse-proxy or VPN setup; set `COOKIE_SECURE=true` when HTTPS is configured.

## Tests and GitHub Actions

### Rust integration tests

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Tests cover anonymous access, invalid passwords, approval changes, logout, password reset, expiry, throttling, missing request headers, video byte ranges, removed media, persistent approvals after rescanning/reopening, symlink escapes, multiple locations, independent approvals for duplicate filenames, invalid location configuration, and migration from the original database schema.

### Container checks

Against a fresh deployment with no approved movies:

```sh
python3 scripts/smoke.py
```

For the full container check, use **an isolated disposable deployment** containing exactly one generated sample video. `scripts/container_check.py` sets a test password, changes approvals, and recreates the container. Never run it against a household deployment.

Example from the repository root, with Docker and FFmpeg installed:

```sh
mkdir -p /tmp/cinema-check-media /tmp/cinema-check-data
ffmpeg -f lavfi -i color=c=blue:s=320x240:d=1 \
  -c:v libx264 -pix_fmt yuv420p /tmp/cinema-check-media/sample.mp4
# Linux only: sudo chown -R 10001:10001 /tmp/cinema-check-data
export COMPOSE_PROJECT_NAME=cinema-check
export HOST_MEDIA_DIR=/tmp/cinema-check-media
export HOST_DATA_DIR=/tmp/cinema-check-data
export PORT=18080
export PLEX_IMAGE=custom-plex:local
docker compose up -d --no-build --pull never --wait
python3 scripts/smoke.py http://127.0.0.1:18080
python3 scripts/container_check.py
docker compose down
unset COMPOSE_PROJECT_NAME HOST_MEDIA_DIR HOST_DATA_DIR PORT PLEX_IMAGE
```

Use new empty test directories for another run. The full check verifies login, approval, streaming, seeking, persistence after container recreation, revocation, and logout. Temporary test credentials remain only in the disposable test database.

### Test multiple Docker locations

After building the local image, run:

```sh
python3 scripts/multi_source_check.py
```

This creates its own temporary folders and isolated Compose project on port 18082, checks distinct streams for identical filenames, independent approvals, persistence after recreation, and removal of a location, then removes its test container and folders. Set `MULTI_SOURCE_TEST_PORT` if 18082 is already in use. It uses synthetic file bytes to test routing; the separate playback sample verifies actual video decoding. GitHub Actions runs this check for both container architectures too.

### Enable GitHub Actions

Push this repository, including `Cargo.lock` and `.github/workflows/ci.yml`, to your GitHub repository and enable Actions. No GitHub remote has been configured in this workspace yet.

The **Verify** workflow runs on pushes and pull requests:

1. Rust formatting, Clippy, and integration tests.
2. Container builds and runtime checks on `ubuntu-latest` (AMD64) and `ubuntu-24.04-arm` (ARM64), using a generated sample video.
3. Container recreation checks using persistent host data.
4. On a `v*` tag, after all tests pass, multi-architecture publication to `ghcr.io/<owner>/<repository>` with version and commit tags.

Native ARM64 runner availability is documented in [GitHub's hosted runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). GitHub Actions usage depends on your repository and account limits.

Set the Rust and both container jobs as required checks in your branch protection/ruleset. Restrict who may create release tags. The publish job uses `GITHUB_TOKEN` with `packages: write`; no personal publishing token is required. Check package visibility/access before pulling onto the Pi. A release image is published by pushing a tag such as `v0.1.0` after committing and pushing the release's code.

CI builds and tests Linux containers. It cannot establish real TV codec support, remote-control behavior, physical disk reliability, or Pi performance. Publishing an image does not deploy it automatically to the Pi.

## Backups, updates, and rollback

Stop the service before copying the database. The commands below use the published-image layout at `/srv/custom-plex`. For the source deployment in `$HOME/custom-plex`, run Compose there and substitute that path for `/srv/custom-plex` in the backup/restore commands. From `/srv/custom-plex`, using its default data path:

```sh
docker compose stop app
sudo tar -czf cinema-data-backup.tar.gz -C /srv/custom-plex data
sudo chmod 600 cinema-data-backup.tar.gz
docker compose up -d --no-build --pull never --wait
```

Move the backup to another device and back up the media separately. The database includes the password hash and session tokens, so keep backups private. Use distinct backup names to retain multiple versions.

For a published-image update, back up first, change `PLEX_IMAGE` in `.env` to the new explicit version or digest, then:

```sh
docker compose pull
docker compose up -d --no-build --pull never --wait
docker compose logs --tail=100 app
curl --fail http://localhost:8080/health
```

Check both profiles and playback afterward. To roll back, select the previous image and restart. If a future release changes the database incompatibly, restore its matching pre-update backup too. The multi-location version automatically migrates the original movie table. To roll back to a pre-migration binary, restore its matching pre-upgrade database backup.

To restore the default data path, stop the service, move the current `data` directory aside, extract the backup into `/srv/custom-plex`, restore ownership to `10001:10001`, and restart. Moving the old directory aside preserves it in case the restore fails. Reset the parents password after restoring to revoke restored sessions.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Cannot connect | Check `docker compose ps`, logs, host port, firewall, and TV network isolation. |
| Port 8080 already in use | Stop the other instance or set a different Compose `PORT` / native `APP_BIND`. |
| Parent login reports unavailable | Run the local `set-password` command against that instance's data directory. |
| Too many login attempts | Wait one minute. Throttling is shared across devices. |
| Empty shelf | Check mount paths, read permissions, scan completion, and Baby approval status. |
| A removed file still appears | Scan again. Scans run automatically only at startup. |
| File will not play | Test an H.264/AAC MP4; prepare incompatible files with FFmpeg. |
| Playback buffers | Check network, storage speed, bitrate, and Pi load. |
| SQLite permission error | Ensure UID 10001 owns the host data directory and can write it. |
| Image cannot be pulled | Check image name/tag, package visibility, and registry login; use a local build until a release is published. |
| Local image unexpectedly pulls | Use `--pull never` after building or explicitly pulling the desired image. |
| `.local` address fails | Use the Pi IP address. |

## Validation status

Cover-image update (September 17, 2026): all eight Rust integration tests and Clippy passed. ARM64 Docker checks verified FFmpeg frame extraction, anonymous image upload for approved movies, persistence across recreation, reset to automatic imagery, and access denial after approval revocation. Browser checks verified Baby's file picker, successful upload and immediate image display, plus automatic cover reset. GitHub Actions runs the extended container checks on its next run; this change has not yet been deployed to the physical Pi.

The multiple-location update passed all six Rust integration tests, formatting, Clippy, and the isolated ARM64 Docker test for distinct streams, approvals, recreation, and source removal. The running local server was updated after a database backup under `backups/before-multisource-20260916T183703Z/data`. That backup is excluded from Git and Docker build context; retain it for rollback to the original schema. No additional personal storage paths are enabled until you configure them.

Verified locally on September 16, 2026:

- Rust formatting and Clippy with warnings denied passed.
- Original integration scenarios passed, covering the access and streaming cases listed above.
- The release Docker image built and ran as `linux/arm64` on Docker Desktop on Apple Silicon.
- HTTP smoke checks and full container checks passed, including persistence after container recreation.
- Browser checks passed for parent login, approval controls, anonymous Baby access, and requiring credentials again after switching profiles. An eight-second generated H.264/AAC MP4 played successfully.

The main local container is available on port 8080 with a generated `Playback-Test.mp4` in the ignored media directory. Its parent account is intentionally unconfigured: run `docker compose exec app custom-plex set-password`, sign in, scan the library, and approve the sample to try Baby playback. The application has no built-in test password; automated tests use a separate disposable database.

Not yet verified: an actual Raspberry Pi/TV, AMD64 runtime locally, GitHub-hosted workflow execution/image publication, and a production backup restore. The GitHub workflow files are ready, but this workspace has no GitHub remote. No hosted test run or published image is claimed.
