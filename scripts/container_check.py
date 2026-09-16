"""Exercise an isolated Compose deployment; never point this at a household library.
Requires one generated sample video, a fresh database, and the app already running.
Uses PORT/COMPOSE_PROJECT_NAME/HOST_DATA_DIR from the caller's environment.
"""
import http.cookiejar
import json
import os
import subprocess
import urllib.error
import urllib.request

base = 'http://127.0.0.1:' + os.environ.get('PORT', '8080')
password = 'disposable-ci-password-2026'
subprocess.run(['docker', 'compose', 'exec', '-T', 'app', 'custom-plex', 'set-password', '--stdin'], input=password+'\n', text=True, check=True)
jar = http.cookiejar.CookieJar()
parent = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))


def request(path, expected=200, data=None, authenticated=False, headers=None):
    values = {'Content-Type': 'application/json', 'X-Requested-With': 'custom-plex'}
    values.update(headers or {})
    req = urllib.request.Request(base + path, data=None if data is None else json.dumps(data).encode(), headers=values)
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
media = f'/media/{movie_id}'
approval = f'/api/movies/{movie_id}/approval'
request(media, 404)
request(approval, 204, {'approved': True}, True)
assert len(json.loads(request('/api/movies'))) == 1
assert len(request(media, 206, headers={'Range': 'bytes=0-31'})) == 32
request(media, 416, headers={'Range': 'bytes=999999999999-'})
subprocess.run(['docker', 'compose', 'up', '-d', '--no-build', '--pull', 'never', '--force-recreate', '--wait'], check=True)
persisted = json.loads(request('/api/movies'))
assert len(persisted) == 1, f'Approval did not persist: {persisted}'
assert json.loads(request('/api/session', authenticated=True))['parent'], 'Session did not persist'
request(approval, 204, {'approved': False}, True)
request(media, 404)
request('/api/logout', 204, {}, True)
request('/api/scan', 401, {}, True)
print('Container checks passed: login, approval, streaming, seeking, recreation, revocation, logout.')
