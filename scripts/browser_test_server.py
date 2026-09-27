"""Start Custom Plex with disposable media and data for Playwright."""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile


root = Path(__file__).resolve().parents[1]
workspace = Path(tempfile.mkdtemp(prefix="custom-plex-browser-"))
media = workspace / "media"
data = workspace / "data"
photos = workspace / "photos"
for directory in (media, data, photos):
    directory.mkdir()

environment = os.environ.copy()
environment.update({
    "APP_BIND": "127.0.0.1:18084",
    "MEDIA_DIR": str(media),
    "MEDIA_SOURCES": "",
    "DATA_DIR": str(data),
    "PHOTO_DIR": str(photos),
    "COOKIE_SECURE": "false",
})

server = None
try:
    subprocess.run(
        [
            "ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i",
            "color=c=blue:s=320x240:d=40", "-c:v", "libx264", "-pix_fmt",
            "yuv420p", "-movflags", "+faststart", str(media / "Browser-Test.mp4"),
        ],
        check=True,
    )
    subprocess.run(["cargo", "build", "--locked"], cwd=root, check=True, env=environment)
    binary = root / "target" / "debug" / "custom-plex"
    subprocess.run(
        [str(binary), "set-password", "--stdin"],
        cwd=root,
        env=environment,
        input="browser-test-password\n",
        text=True,
        check=True,
    )
    server = subprocess.Popen([str(binary)], cwd=root, env=environment)

    def stop(_signal, _frame):
        """Forward Playwright's shutdown signal to the disposable Rust server."""
        if server.poll() is None:
            server.terminate()

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    raise SystemExit(server.wait())
finally:
    if server is not None and server.poll() is None:
        server.terminate()
        server.wait(timeout=10)
    shutil.rmtree(workspace, ignore_errors=True)
