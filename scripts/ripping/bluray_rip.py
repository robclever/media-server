#!/usr/bin/env python3
"""Rip an authorized DVD, Blu-ray, or UHD Blu-ray title to MKV using MakeMKV."""

from __future__ import annotations

import argparse
import csv
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Title:
    number: int
    name: str = ""
    duration: str = ""
    size_bytes: int = 0
    playlist: str = ""

    @property
    def size_gib(self) -> float:
        return self.size_bytes / (1024**3)


def robot_fields(line: str) -> list[str]:
    """Parse MakeMKV's comma-separated robot output, including quoted commas."""
    return next(csv.reader([line], skipinitialspace=False, escapechar="\\"))


def parse_titles(output: str) -> list[Title]:
    fields: dict[int, dict[int, str]] = {}
    for line in output.splitlines():
        if not line.startswith("TINFO:"):
            continue
        try:
            title_no, info_code, _flags, value = robot_fields(line[6:])[:4]
            fields.setdefault(int(title_no), {})[int(info_code)] = value
        except (ValueError, IndexError):
            continue

    titles = []
    for number, info in fields.items():
        try:
            size = int(info.get(11, "0"))  # source file size in bytes
        except ValueError:
            size = 0
        titles.append(
            Title(
                number=number,
                name=info.get(2, ""),       # title name
                duration=info.get(9, ""),   # duration
                size_bytes=size,
                playlist=info.get(16, ""),  # playlist file name
            )
        )
    return sorted(titles, key=lambda title: title.number)


def makemkv() -> str:
    executable = shutil.which("makemkvcon")
    if executable:
        return executable
    candidates = [
        Path("/Applications/MakeMKV.app/Contents/MacOS/makemkvcon"),
        Path("/usr/bin/makemkvcon"),
        Path("/usr/local/bin/makemkvcon"),
        Path("/opt/homebrew/bin/makemkvcon"),
    ]
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    # Also support running directly from the official macOS installer image.
    for candidate in sorted(
        Path("/Volumes").glob("makemkv_v*/MakeMKV.app/Contents/MacOS/makemkvcon"),
        reverse=True,
    ):
        if candidate.is_file():
            return str(candidate)
    raise SystemExit(
        "MakeMKV was not found. Install the no-cost beta from "
        "https://www.makemkv.com/download/ and then run this command again."
    )


def ffmpeg() -> str:
    executable = shutil.which("ffmpeg")
    if executable:
        return executable
    raise SystemExit(
        "FFmpeg was not found. On macOS, install it with: brew install ffmpeg"
    )


def conversion_command(
    source: Path,
    destination: Path,
    quality: int,
    overwrite: bool,
    resolution: str = "source",
) -> list[str]:
    command = [
        ffmpeg(),
        "-hide_banner",
        "-y" if overwrite else "-n",
        "-i",
        str(source),
        "-map",
        "0:v:0",
        "-map",
        "0:a:0?",
        "-map_metadata",
        "0",
    ]
    if resolution == "1080p":
        command.extend(
            [
                "-vf",
                "scale=1920:1080:force_original_aspect_ratio=decrease:flags=lanczos,"
                "pad=1920:1080:(ow-iw)/2:(oh-ih)/2,setsar=1",
            ]
        )
    command.extend([
        "-c:v",
        "libx264",
        "-preset",
        "medium",
        "-crf",
        str(quality),
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-movflags",
        "+faststart",
        str(destination),
    ])
    return command


def convert(
    source: Path,
    output: Path | None,
    quality: int,
    overwrite: bool,
    resolution: str,
) -> None:
    source = source.expanduser().resolve()
    if not source.is_file():
        raise SystemExit(f"Input file does not exist: {source}")
    if source.suffix.lower() != ".mkv":
        raise SystemExit("The convert command expects an .mkv input file.")
    destination = (output or source.with_suffix(".mp4")).expanduser().resolve()
    if destination == source:
        raise SystemExit("Input and output paths must be different.")
    if destination.exists() and not overwrite:
        raise SystemExit(
            f"Output already exists: {destination}\nUse --overwrite to replace it."
        )
    destination.parent.mkdir(parents=True, exist_ok=True)
    print(f"Converting {source.name} to Apple-compatible MP4...")
    try:
        completed = subprocess.run(
            conversion_command(source, destination, quality, overwrite, resolution)
        )
    except KeyboardInterrupt:
        raise SystemExit("\nConversion cancelled; a partial output may remain.") from None
    if completed.returncode != 0:
        raise SystemExit(f"FFmpeg failed with exit code {completed.returncode}.")
    print(f"Conversion complete: {destination}")


def inspect_source(
    source: str, min_length: int
) -> tuple[list[Title], subprocess.CompletedProcess[str]]:
    command = [
        makemkv(),
        "-r",
        "--cache=1",
        "--progress=-same",
        f"--minlength={min_length}",
        "info",
        source,
    ]
    process = subprocess.Popen(
        command,
        stdin=subprocess.DEVNULL,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        bufsize=1,
    )
    output_lines: list[str] = []
    assert process.stdout is not None
    # Display errors as well as status; only omit repetitive short-title notices.
    last_percent = -1
    for line in scan_lines(process):
        output_lines.append(line)
        if line.startswith(("PRGT:", "PRGC:")):
            print(f"  {robot_fields(line[5:])[-1]}", file=sys.stderr, flush=True)
        elif line.startswith("PRGV:"):
            try:
                current, total, maximum = map(int, robot_fields(line[5:]))
                percent = total * 100 // maximum if maximum else 0
                if percent != last_percent:
                    print(f"  Scan progress: {percent}%", file=sys.stderr, flush=True)
                    last_percent = percent
            except ValueError:
                pass
        if line.startswith("MSG:"):
            try:
                message = robot_fields(line[4:])
                if int(message[0]) != 3025:
                    print(f"  {message[3]}", file=sys.stderr, flush=True)
            except (ValueError, IndexError):
                pass
    return_code = process.wait()
    result = subprocess.CompletedProcess(
        command,
        return_code,
        stdout="".join(output_lines),
        stderr="",
    )
    return parse_titles(result.stdout), result


def scan_lines(process):
    """Stop the scanner when the user interrupts the wrapper."""
    try:
        yield from process.stdout
    except KeyboardInterrupt:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        raise SystemExit("\nScan cancelled; MakeMKV stopped.") from None
    finally:
        process.stdout.close()


def mounted_dvd_sources() -> list[str]:
    """Return mounted DVD-Video roots for discs needing folder-mode fallback."""
    if sys.platform != "darwin":
        return []
    return [
        f"file:{video_ts.parent}"
        for video_ts in sorted(Path("/Volumes").glob("*/VIDEO_TS"))
        if video_ts.is_dir()
    ]


def scan_source(drive: int, min_length: int = 2) -> tuple[list[Title], str]:
    disc_source = f"disc:{drive}"
    titles, result = inspect_source(disc_source, min_length)
    if titles:
        return titles, disc_source

    # Some remastered/burned DVDs have navigation metadata that MakeMKV rejects
    # in raw-disc mode but can recover when given the mounted DVD-Video root.
    for source in mounted_dvd_sources():
        titles, folder_result = inspect_source(source, min_length)
        if titles:
            print(f"Note: using mounted DVD fallback {source[5:]}", file=sys.stderr)
            return titles, source
        if folder_result.returncode != 0:
            result = folder_result

    detail = result.stderr.strip() or result.stdout.strip()
    suffix = f"\n{detail}" if detail else ""
    raise SystemExit(f"No titles were found on disc {drive}.{suffix}")


def print_titles(titles: list[Title]) -> None:
    print(f"{'Title':>5}  {'Size':>9}  {'Duration':>10}  {'Playlist':>12}  Name")
    for title in titles:
        print(
            f"{title.number:>5}  {title.size_gib:>7.2f} GB  "
            f"{title.duration or '-':>10}  {title.playlist or '-':>12}  {title.name or '-'}"
        )


def choose_title(titles: list[Title], requested: int | None) -> Title:
    if requested is None:
        return max(titles, key=lambda title: title.size_bytes)
    for title in titles:
        if title.number == requested:
            return title
    available = ", ".join(str(title.number) for title in titles)
    raise SystemExit(f"Title {requested} was not found. Available titles: {available}")


def rip(drive: int, output: Path, title_no: int | None, min_length: int) -> None:
    titles, source = scan_source(drive, min_length)
    print_titles(titles)
    title = choose_title(titles, title_no)
    output = output.expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)
    print(
        f"\nRipping title {title.number} ({title.size_gib:.2f} GB, "
        f"{title.duration or 'unknown duration'}) to {output}"
    )
    command = [
        makemkv(),
        "--progress=-same",
        f"--minlength={min_length}",
        "mkv",
        source,
        str(title.number),
        str(output),
    ]
    try:
        completed = subprocess.run(command, stdin=subprocess.DEVNULL)
    except KeyboardInterrupt:
        raise SystemExit("\nRip cancelled.") from None
    if completed.returncode != 0:
        raise SystemExit(f"MakeMKV failed with exit code {completed.returncode}.")
    print(f"\nRip complete. Output directory: {output}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Rip a DVD, Blu-ray, or UHD Blu-ray you are authorized to copy to MKV."
    )
    parser.add_argument("--drive", type=int, default=0, help="MakeMKV drive number (default: 0)")
    subparsers = parser.add_subparsers(dest="command", required=True)
    scan_parser = subparsers.add_parser("scan", help="list the titles found on the disc")
    scan_parser.add_argument(
        "--min-length",
        type=int,
        default=2,
        help="ignore shorter titles, in seconds (default: 2)",
    )
    rip_parser = subparsers.add_parser("rip", help="rip one title without re-encoding")
    rip_parser.add_argument("output", type=Path, help="output directory")
    rip_parser.add_argument(
        "--title", type=int, help="title number; defaults to the largest title"
    )
    rip_parser.add_argument(
        "--min-length",
        type=int,
        default=2,
        help="ignore shorter titles, in seconds (default: 2)",
    )
    convert_parser = subparsers.add_parser(
        "convert", help="convert an MKV to an Apple-compatible H.264/AAC MP4"
    )
    convert_parser.add_argument("input", type=Path, help="source .mkv file")
    convert_parser.add_argument(
        "output", type=Path, nargs="?", help="output .mp4; defaults beside the input"
    )
    convert_parser.add_argument(
        "--quality",
        type=int,
        choices=range(16, 29),
        default=20,
        metavar="16-28",
        help="H.264 CRF quality; lower is better/larger (default: 20)",
    )
    convert_parser.add_argument(
        "--overwrite", action="store_true", help="replace an existing output file"
    )
    convert_parser.add_argument(
        "--resolution",
        choices=("source", "1080p"),
        default="source",
        help="keep source dimensions or upscale/pad to 1920x1080 (default: source)",
    )
    return parser


def main() -> None:
    args = build_parser().parse_args()
    if args.command == "scan":
        titles, _source = scan_source(args.drive, args.min_length)
        print_titles(titles)
    elif args.command == "rip":
        rip(args.drive, args.output, args.title, args.min_length)
    else:
        convert(args.input, args.output, args.quality, args.overwrite, args.resolution)


if __name__ == "__main__":
    main()
