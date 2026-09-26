"""Exercise an isolated Compose deployment; never point this at a household library.
Requires one generated sample video, a fresh database, and the app already running.
Uses PORT/COMPOSE_PROJECT_NAME/HOST_DATA_DIR from the caller's environment.
"""
import http.cookiejar
import json
import os
import subprocess
import struct
import zlib
import urllib.error
import urllib.request

base = 'http://127.0.0.1:' + os.environ.get('PORT', '8080')
password = 'disposable-ci-password-2026'
subprocess.run(['docker', 'compose', 'exec', '-T', 'app', 'custom-plex', 'set-password', '--stdin'], input=password+'\n', text=True, check=True)
jar = http.cookiejar.CookieJar()
parent = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))


def request(path, expected=200, data=None, authenticated=False, headers=None, method=None):
    values = {'Content-Type': 'application/json', 'X-Requested-With': 'custom-plex'}
    values.update(headers or {})
    req = urllib.request.Request(base + path, data=data if isinstance(data, bytes) else None if data is None else json.dumps(data).encode(), headers=values, method=method)
    try:
        result = (parent.open if authenticated else urllib.request.urlopen)(req, timeout=10)
    except urllib.error.HTTPError as error:
        result = error
    assert result.status == expected, (path, result.status, expected)
    return result.read()


assert json.loads(request('/api/movies')) == []
request('/api/login', 204, {'password': password}, True)
movies = json.loads(request('/api/movies', authenticated=True))
assert len(movies) == 1, 'Use an isolated library with exactly one sample movie'
movie_id = movies[0]['id']
request(f'/api/movies/{movie_id}/title', 204, {'name': 'Renamed sample'}, True)
assert json.loads(request('/api/movies', authenticated=True))[0]['title'] == 'Renamed sample'
media = f'/media/{movie_id}'
approval = f'/api/movies/{movie_id}/approval'
request(media, 404)
request(approval, 204, {'approved': True}, True)
assert len(json.loads(request('/api/movies'))) == 1
assert len(request(media, 206, headers={'Range': 'bytes=0-31'})) == 32
request(media, 416, headers={'Range': 'bytes=999999999999-'})
cover = f'/api/movies/{movie_id}/cover'
generated = request(cover)
assert generated.startswith(b'\xff\xd8'), 'FFmpeg did not produce a JPEG cover'
def png_chunk(kind, value):
    return struct.pack('!I', len(value)) + kind + value + struct.pack('!I', zlib.crc32(kind + value))
png = (b'\x89PNG\r\n\x1a\n' + png_chunk(b'IHDR', struct.pack('!2I5B', 1, 1, 8, 2, 0, 0, 0))
       + png_chunk(b'IDAT', zlib.compress(b'\x00\xff\x00\x00')) + png_chunk(b'IEND', b''))
# Baby can change the image of an approved movie without a parent session.
request(cover, 204, png, headers={'Content-Type': 'image/png'})
uploaded = request(cover)
assert uploaded.startswith(b'\xff\xd8') and uploaded != generated

# Photo Album creation and upload require no parent session.
album = json.loads(request('/api/albums', 201, {'name': 'Family photos'}))
photo = json.loads(request(f"/api/albums/{album['id']}/photos?name=sample.png", 201, png, headers={'Content-Type': 'image/png'}))
destination = json.loads(request('/api/albums', 201, {'name': 'Recent album'}))
request(f"/api/photos/{photo['id']}/name", 204, {'name': 'Renamed photo'})
request(f"/api/photos/{photo['id']}/album", 204, {'album_id': destination['id']})
request(f"/api/albums/{destination['id']}/name", 204, {'name': 'Moved photos'})
request(f"/api/albums/{album['id']}", 204, method='DELETE')
original_url = f"/api/photos/{photo['id']}/original"
assert request(original_url) == png
assert request(f"/api/photos/{photo['id']}/thumbnail").startswith(b'\xff\xd8')
storage = json.loads(request('/api/storage'))
assert storage['total'] > 0 and storage['available'] <= storage['total'] and storage['volumes']

subprocess.run(['docker', 'compose', 'up', '-d', '--no-build', '--pull', 'never', '--force-recreate', '--wait'], check=True)
assert request(original_url) == png, 'Photo original did not persist'
albums = json.loads(request('/api/albums'))
assert any(a['id'] == destination['id'] and a['name'] == 'Moved photos' and a['count'] == 1 for a in albums), 'Album management did not persist'

persisted = json.loads(request('/api/movies'))
assert len(persisted) == 1, f'Approval did not persist: {persisted}'
assert json.loads(request('/api/session', authenticated=True))['parent'], 'Session did not persist'
# A persisted session alone cannot prove the password hash survived deployment.
request('/api/logout', 204, {}, True)
jar.clear()
request('/api/login', 204, {'password': password}, True)
assert json.loads(request('/api/session', authenticated=True))['parent'], 'Password did not persist'
assert request(cover) == uploaded, 'Uploaded cover did not persist'
request(cover, 204, method='DELETE')
assert request(cover) == generated, 'Automatic cover did not return after reset'
request(approval, 204, {'approved': False}, True)
request(media, 404)
request(cover, 404)
request(f"/api/photos/{photo['id']}", 204, method='DELETE')
request(original_url, 404)
request(f"/api/albums/{destination['id']}", 204, method='DELETE')
request('/api/logout', 204, {}, True)
request('/api/scan', 401, {}, True)
print('Container checks passed: login, movie rename, approval, streaming, seeking, covers, photo album management, storage usage, recreation, password persistence, deletion, revocation, logout.')
