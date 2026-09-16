"""Test two read-only media mounts in a disposable Compose project."""
import http.cookiejar
import json
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.error
import urllib.request

repo = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="cinema-sources-") as temporary:
    root = Path(temporary)
    root.chmod(0o755)
    for folder in ["media", "archive", "data"]:
        (root / folder).mkdir()
        (root / folder).chmod(0o777 if folder == "data" else 0o755)
    (root / "media/Same.mp4").write_bytes(b"first-location")
    (root / "archive/Same.mp4").write_bytes(b"second-location")
    env = dict(os.environ, COMPOSE_PROJECT_NAME=root.name,
               HOST_MEDIA_DIR=str(root / "media"), HOST_ARCHIVE_DIR=str(root / "archive"),
               HOST_DATA_DIR=str(root / "data"), PORT=os.environ.get("MULTI_SOURCE_TEST_PORT", "18082"),
               PLEX_IMAGE=os.environ.get("PLEX_IMAGE", "custom-plex:local"),
               MEDIA_SOURCES=json.dumps({"default": "/media", "archive": "/media-archive"}),
               COOKIE_SECURE="false")
    compose = ["docker", "compose", "-f", str(repo / "compose.yaml"), "-f", str(repo / "compose.storage.example.yaml")]

    def docker(*args, **kwargs):
        return subprocess.run(compose + list(args), env=env, check=True, **kwargs)

    jar = http.cookiejar.CookieJar()
    parent = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    base = "http://127.0.0.1:" + env["PORT"]

    def request(path, expected=200, data=None, authenticated=False):
        req = urllib.request.Request(base + path, data=None if data is None else json.dumps(data).encode(),
                                     headers={"Content-Type": "application/json", "X-Requested-With": "custom-plex"})
        try:
            response = (parent.open if authenticated else urllib.request.urlopen)(req, timeout=10)
        except urllib.error.HTTPError as error:
            response = error
        assert response.status == expected, (path, response.status, expected)
        return response.read()

    try:
        docker("up", "-d", "--no-build", "--pull", "never", "--wait")
        password = "disposable-source-test-password"
        docker("exec", "-T", "app", "custom-plex", "set-password", "--stdin", input=password + "\n", text=True)
        request("/api/login", 204, {"password": password}, True)
        movies = json.loads(request("/api/movies", authenticated=True))
        assert len(movies) == 2
        by_source = {movie["source"]: movie for movie in movies}
        assert set(by_source) == {"default", "archive"}
        assert json.loads(request("/api/movies")) == []
        for name, content in [("default", b"first-location"), ("archive", b"second-location")]:
            path = f'/media/{by_source[name]["id"]}'
            request(path, 404)
            assert request(path, authenticated=True) == content
        archive_id = by_source["archive"]["id"]
        request(f"/api/movies/{archive_id}/approval", 204, {"approved": True}, True)
        docker("up", "-d", "--no-build", "--pull", "never", "--force-recreate", "--wait")
        visible = json.loads(request("/api/movies"))
        assert len(visible) == 1 and visible[0]["id"] == archive_id
        env["MEDIA_SOURCES"] = json.dumps({"default": "/media"})
        docker("up", "-d", "--no-build", "--pull", "never", "--wait")
        assert json.loads(request("/api/movies")) == []
        request(f"/media/{archive_id}", 404, authenticated=True)
        print("Multiple-location container checks passed: distinct streams, independent approvals, persistence, removal.")
    finally:
        docker("logs", "--no-color", "--tail=30")
        docker("down")
