#!/usr/bin/env python3
"""Publish a finished movie to Custom Plex's My Passport library."""

from __future__ import annotations

import argparse
import hashlib
import shlex
import subprocess
import sys
import uuid
from pathlib import Path


DEFAULT_HOST = "192.168.0.73"
DEFAULT_USER = "rob"
DEFAULT_KEY = Path.home() / ".ssh" / "custom_plex_pi"
DEFAULT_REMOTE_MEDIA = Path("/mnt/dvd-library/Custom-Plex-Movies")
SUPPORTED_EXTENSIONS = {".mp4", ".m4v", ".webm", ".mov", ".mkv"}
SCRIPT_DIR = Path(__file__).resolve().parent


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as movie:
        for block in iter(lambda: movie.read(4 * 1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def run_ssh(host: str, user: str, key: Path, command: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "ssh",
            "-i",
            str(key),
            "-o",
            "BatchMode=yes",
            f"{user}@{host}",
            command,
        ],
        check=True,
        text=True,
        capture_output=True,
    )


def publish(
    source: Path,
    name: str | None,
    host: str,
    user: str,
    key: Path,
    remote_media: Path,
    move_source: bool,
) -> None:
    source = source.expanduser()
    if not source.is_absolute():
        source = SCRIPT_DIR / source
    source = source.resolve()
    key = key.expanduser().resolve()

    if not source.is_file():
        raise SystemExit(f"Movie does not exist: {source}")
    if source.suffix.lower() not in SUPPORTED_EXTENSIONS:
        allowed = ", ".join(sorted(SUPPORTED_EXTENSIONS))
        raise SystemExit(f"Unsupported movie extension. Expected one of: {allowed}")
    if not key.is_file():
        raise SystemExit(f"SSH key does not exist: {key}")

    filename = name or source.name
    if filename in {"", ".", ".."} or Path(filename).name != filename:
        raise SystemExit("--name must be a filename, not a path.")
    if Path(filename).suffix.lower() not in SUPPORTED_EXTENSIONS:
        raise SystemExit("--name must keep a supported video extension.")

    remote_final = remote_media / filename
    remote_temporary = remote_media / f".{filename}.{uuid.uuid4().hex}.uploading"
    quoted_dir = shlex.quote(str(remote_media))
    quoted_final = shlex.quote(str(remote_final))
    quoted_temporary = shlex.quote(str(remote_temporary))

    mount_source = run_ssh(
        host,
        user,
        key,
        f"set -eu; test -d {quoted_dir}; findmnt -rn -T {quoted_dir} -o SOURCE; "
        f"test ! -e {quoted_final}",
    ).stdout.strip()
    if not mount_source.startswith("/dev/sda1"):
        raise SystemExit(
            f"Refusing upload: {remote_media} is mounted from {mount_source!r}, not My Passport."
        )
    print(f"Uploading to {user}@{host}:{remote_final} ...", flush=True)
    upload_command = (
        f"set -eu; cat > {quoted_temporary}; chmod 644 {quoted_temporary}; "
        f"mv {quoted_temporary} {quoted_final}"
    )
    with source.open("rb") as movie:
        completed = subprocess.run(
            [
                "ssh",
                "-i",
                str(key),
                "-o",
                "BatchMode=yes",
                f"{user}@{host}",
                upload_command,
            ],
            stdin=movie,
        )
    if completed.returncode != 0:
        raise SystemExit(f"Upload failed with exit code {completed.returncode}.")

    remote_size = int(
        run_ssh(host, user, key, f"stat -c %s {quoted_final}").stdout.strip()
    )
    if remote_size != source.stat().st_size:
        raise SystemExit(
            f"Upload size mismatch: local {source.stat().st_size}, remote {remote_size}"
        )

    local_digest = sha256(source)
    remote_digest = run_ssh(host, user, key, f"sha256sum {quoted_final}").stdout.split()[0]
    if remote_digest != local_digest:
        raise SystemExit("Upload checksum mismatch; the server copy may be corrupt.")

    if move_source:
        source.unlink()
        print(f"Removed original after the Passport upload was verified: {source}")

    print(f"Published successfully: {filename}")
    print("In Custom Plex, sign in as Parents and click Scan library.")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Upload a finished movie to Custom Plex's My Passport library."
    )
    parser.add_argument(
        "movie",
        type=Path,
        help=(
            "finished video file; relative paths start from the "
            "scripts/ripping folder"
        ),
    )
    parser.add_argument(
        "--name",
        help='library filename, for example "The Room (2003).mp4"',
    )
    parser.add_argument(
        "--move-source",
        action="store_true",
        help="delete the original only after the Passport copy verifies",
    )
    parser.add_argument("--host", default=DEFAULT_HOST)
    parser.add_argument("--user", default=DEFAULT_USER)
    parser.add_argument("--key", type=Path, default=DEFAULT_KEY)
    parser.add_argument("--remote-media", type=Path, default=DEFAULT_REMOTE_MEDIA)
    args = parser.parse_args()
    publish(
        args.movie,
        args.name,
        args.host,
        args.user,
        args.key,
        args.remote_media,
        args.move_source,
    )


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() if error.stderr else str(error)
        raise SystemExit(f"Remote command failed: {detail}") from error
