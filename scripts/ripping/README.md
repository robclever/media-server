# DVD and Blu-ray ripping tools

A free-to-use macOS/Linux command-line tool for copying a DVD, Blu-ray, or UHD
Blu-ray that you own or are otherwise authorized to copy. It uses MakeMKV and keeps the
original video, HDR metadata, audio tracks, chapters, and subtitles without
quality loss or re-encoding.

Run the examples below from the Custom Plex repository root. The tools live in
`scripts/ripping/`, and their default working output is
`scripts/ripping/output/`. That output directory is excluded from Git because
disc images and converted movies are large.

## Cost and open-source status

The script is free. MakeMKV's current beta can also be used at no cost and its
official download page resets the trial when a new beta is installed. MakeMKV is
proprietary, however. It is used here because the fully open-source alternatives
(`libbluray`, `libaacs`, and FFmpeg) cannot reliably decrypt arbitrary commercial
4K UHD discs without separately supplied disc keys.

If your source is unencrypted or has already been decrypted, FFmpeg is a fully
open-source remuxing option, but that is a different workflow from reading a
typical retail UHD disc.

## Requirements

- An optical drive that can read the disc. UHD discs require a compatible UHD
  drive/firmware; ordinary DVD drives work for DVDs.
- The [no-cost MakeMKV beta](https://www.makemkv.com/download/). On macOS, install the app in
  `/Applications`; the script finds its bundled `makemkvcon` automatically.
- Enough free disk space. A UHD feature commonly requires tens of gigabytes.

This project does not include keys, firmware, or decryption code. Laws governing
copying and access-control circumvention vary by location; use it only where you
have authorization and doing so is legal.

## Usage

Insert a DVD or Blu-ray and list its titles:

```sh
python3 scripts/ripping/bluray_rip.py scan
```

Titles as short as two seconds are included by default. Change the threshold if
needed with `scan --min-length 1` or `rip ... --min-length 1`.

Rip the largest title (usually the main feature):

```sh
python3 scripts/ripping/bluray_rip.py rip "/path/to/output-folder"
```

Or select a title shown by `scan`:

```sh
python3 scripts/ripping/bluray_rip.py rip "/path/to/output-folder" --title 3
```

For a second optical drive, place the global option before the command:

```sh
python3 scripts/ripping/bluray_rip.py --drive 1 scan
```

### Rip and convert in one command

On macOS/Linux, the included shell script selects the largest title on the disc
and converts it to a native-resolution H.264/AAC MP4:

```sh
./scripts/ripping/rip_movie.sh
```

The initial scan may take several minutes on DVDs with complex menus. The script
prints MakeMKV scan activity as it works; wait until it displays the title list
and begins ripping.

By default it creates a timestamped folder under `scripts/ripping/output`. You
can provide an output directory and a different title number:

```sh
./scripts/ripping/rip_movie.sh "/path/to/output" 3
```

MakeMKV names the resulting `.mkv` file. The copy is lossless; playback devices
that cannot decode the original MPEG-2/HEVC or audio formats may require a later
FFmpeg or HandBrake conversion. Episodic DVDs commonly contain several similarly
sized titles, so use `scan` and `--title` instead of relying on the largest-title
default for those discs.

## Convert for Apple devices

Install free FFmpeg once (`brew install ffmpeg` on macOS), then convert an MKV
to an MP4 that plays natively in QuickTime, Apple TV, iPhone, and iPad:

```sh
python3 scripts/ripping/bluray_rip.py convert "/path/to/movie.mkv"
```

The MP4 is written next to the MKV. The source is never deleted. The default
uses H.264 video, AAC audio, and broadly compatible 8-bit 4:2:0 pixels. Use
`--quality 18` for a larger/higher-quality file or `--quality 23` for a smaller
file. Existing output files are protected unless `--overwrite` is supplied.

To create a standard 1920x1080 file while preserving the source aspect ratio:

```sh
python3 scripts/ripping/bluray_rip.py convert movie.mkv movie_1080p.mp4 --resolution 1080p
```

Upscaling does not restore detail absent from a DVD; a 4:3 DVD is scaled to
1440x1080 and padded with black side bars instead of being stretched.

## Archive and publish to Custom Plex

The external drive named `My Passport` is mounted on the Pi at
`/mnt/dvd-library`. The publish script uploads directly into its
`Custom-Plex-Movies` folder through the Custom Plex server at `192.168.0.73`.
The server receives a hidden `.uploading` file first, so an interrupted transfer
cannot appear as a playable movie. The script refuses to upload if that folder
is not mounted from the expected Passport device.

Pass the movie as a path relative to `scripts/ripping`. This works even when the
command is launched from another directory:

```sh
python3 scripts/ripping/publish_movie.py \
  "output/movie_2026-09-16_15-01-44/The_Room.mp4" \
  --name "The Room (2003).mp4"
```

The default keeps the original rip output. To move it out of `output` after the
Passport copy passes size and SHA-256 verification, add `--move-source`:

```sh
python3 scripts/ripping/publish_movie.py "output/movie_folder/movie.mp4" \
  --name "Movie Title (2026).mp4" --move-source
```

Existing archive or server filenames are never overwritten. After publishing,
open Custom Plex, sign in as Parents, and click **Scan library**. The new movie
is parents-only until it is explicitly added to Baby.

## Test

```sh
python3 -m unittest discover -s scripts/ripping -p 'test_*.py' -v
```
