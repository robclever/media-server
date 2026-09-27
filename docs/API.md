# HTTP API reference

The browser and server use same-origin JSON endpoints. Unless stated otherwise, request bodies are JSON and successful mutation responses use `204 No Content`.

All `POST` and `DELETE` requests require:

```http
X-Requested-With: custom-plex
```

Missing this header returns `403 Forbidden`. Parents authentication uses the `session` cookie returned by login.

## Session and library

| Method | Path | Access | Request or response |
| --- | --- | --- | --- |
| `GET` | `/health` | Public | Plain text `ok`. |
| `GET` | `/api/session` | Public | `{"parent": boolean}`. |
| `POST` | `/api/login` | Public | Request `{"password":"..."}`. Sets an eight-hour session cookie. |
| `POST` | `/api/logout` | Public | Deletes the current session and expires its cookie. |
| `GET` | `/api/movies` | Public | Baby receives approved present movies; Parents receives every present movie. Each item has `id`, `title`, `source`, `approved`, `position`, and `duration`. |
| `POST` | `/api/scan` | Parents | Recursively rescans configured sources and returns the total movie count as JSON. |
| `POST` | `/api/movies/{id}/approval` | Parents | `{"approved":true}` makes a movie visible to Baby; `false` revokes it. |
| `POST` | `/api/movies/{id}/title` | Parents | `{"name":"Display title"}`; 1–255 non-control characters after trimming. |
| `GET`, `HEAD` | `/media/{id}` | Visible movie | Streams the file and supports byte ranges. Hidden or absent movies return `404`. |

## Playback progress

`POST /api/movies/{id}/progress` accepts a visible movie and:

```json
{
  "position": 937.25,
  "duration": 5420.8
}
```

Values are seconds. Both must be finite; position must be nonnegative and duration must be between zero and seven days. Position is clamped to duration. Positions in the first second or final 15 seconds remove saved progress, causing the next play to start at the beginning. Progress is shared across profiles and devices.

## Movie covers

| Method | Path | Access | Behavior |
| --- | --- | --- | --- |
| `GET` | `/api/movies/{id}/cover` | Visible movie | Returns JPEG. Resolution order is uploaded image, same-stem sidecar image, extracted video frame, then `404`. |
| `POST` | `/api/movies/{id}/cover` | Visible movie | Raw JPEG, PNG, or WebP body, up to 8 MiB and 8192 pixels per side. Normalizes to JPEG. |
| `DELETE` | `/api/movies/{id}/cover` | Visible movie | Removes the uploaded/cache record so automatic selection runs again. |

## Photo albums

Photo routes require no password. This is intentional for the trusted-home-network Photo Album profile.

| Method | Path | Request or response |
| --- | --- | --- |
| `GET` | `/api/albums` | Albums ordered by recent use: `id`, `name`, `description`, `count`, and `cover_id`. |
| `POST` | `/api/albums` | `{"name":"Album"}`; returns the created album with `201 Created`. |
| `DELETE` | `/api/albums/{id}` | Permanently deletes the album and every stored photo variant. |
| `POST` | `/api/albums/{id}/name` | `{"name":"New name"}`. |
| `POST` | `/api/albums/{id}/description` | `{"description":"..."}`; up to 2,000 characters. Empty clears it. |
| `GET` | `/api/albums/{id}/photos` | Photo records with `id`, `name`, and `description`. Opening the list marks the album recently used. |
| `POST` | `/api/albums/{id}/photos?name=...` | Raw JPEG, PNG, or WebP body. Maximum 24 MiB and 8192 pixels per side. Returns `201 Created`. |
| `POST` | `/api/photos/{id}/name` | `{"name":"New name"}`. |
| `POST` | `/api/photos/{id}/description` | `{"description":"..."}`; up to 2,000 characters. Empty clears it. |
| `POST` | `/api/photos/{id}/album` | `{"album_id":2}` moves the photo. |
| `DELETE` | `/api/photos/{id}` | Permanently deletes all stored variants and metadata. |
| `GET` | `/api/photos/{id}/thumbnail` | JPEG up to 400 pixels. |
| `GET` | `/api/photos/{id}/preview` | JPEG up to 1600 pixels. |
| `GET` | `/api/photos/{id}/original` | Original upload bytes with an attachment disposition. |

## Storage

`GET /api/storage` returns total and available bytes across the application data directory, photo directory, and movie roots. Filesystem device IDs prevent double-counting multiple paths on the same disk.

```json
{
  "total": 1000000,
  "available": 400000,
  "volumes": [
    {"name":"App data","total":1000000,"available":400000}
  ]
}
```

## Common status codes

| Status | Meaning |
| --- | --- |
| `400` | Invalid name, description, or playback value. |
| `401` | Parents session required or invalid password. |
| `403` | State-changing request omitted the required custom header. |
| `404` | Resource does not exist or the current profile may not see it. |
| `413` | Upload exceeds its route limit. |
| `415` | Uploaded image is unsupported or invalid. |
| `429` | Login-attempt limit reached. |
| `500` | Database, filesystem, or processing failure. |
| `503` | Parents password has not been configured. |
