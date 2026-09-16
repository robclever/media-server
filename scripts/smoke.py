"""Run against a fresh container: python3 scripts/smoke.py [base-url]."""
import json
import sys
import urllib.error
import urllib.request

base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"

def request(path, expected=200, body=None):
    req = urllib.request.Request(base + path, data=body, headers={"X-Requested-With": "custom-plex", "Content-Type": "application/json"})
    try:
        response = urllib.request.urlopen(req, timeout=10)
    except urllib.error.HTTPError as error:
        response = error
    assert response.status == expected, (path, response.status, expected)
    return response.read()

assert request("/health") == b"ok"
assert b"Family Cinema" in request("/")
assert json.loads(request("/api/movies")) == []
assert json.loads(request("/api/session")) == {"parent": False}
request("/api/scan", 401, b"{}")
request("/media/1", 404)
print("Container smoke checks passed: health, UI, anonymous restrictions.")
