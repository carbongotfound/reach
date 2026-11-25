"""A fake GitHub Releases API for testing the app's update check by hand.

  python -I tests/updater/mock.py <port> [scenario] [installer.exe]
  $env:REACH_UPDATE_API = "http://127.0.0.1:<port>"; target\\release\\reach-app.exe

Scenarios: update (default: 0.3.1 is out), none (only 0.3.0), pre (only 0.4.0-beta.1, a pre-release),
bad (0.3.1 whose SHA256SUMS lists the wrong hash), error (HTTP 500), limited (rate limited for an hour).
The installer served is the given file, or a small dummy that is not a real installer.
Loopback only. Stop it by PID (it prints its PID on start).
"""
import hashlib, http.server, json, os, sys, time

port = int(sys.argv[1])
scenario = sys.argv[2] if len(sys.argv) > 2 else 'update'
payload = open(sys.argv[3], 'rb').read() if len(sys.argv) > 3 else b'MZ dummy installer for the update test, never run'
base = f'http://127.0.0.1:{port}'
NOTES = 'Fixes\n- The pill fades out after Done.\n- <b>HTML here must show as text</b>\n\nThanks for testing the beta.'


def rel(tag, pre=False):
    v = tag.lstrip('v')
    name = f'Reach_{v}_x64-setup.exe'
    return {'tag_name': tag, 'name': f'Reach {tag}', 'draft': False, 'prerelease': pre, 'published_at': '2026-11-24T09:00:00Z',
            'body': NOTES, 'html_url': f'https://github.com/carbongotfound/reach/releases/tag/{tag}',
            'assets': [{'name': name, 'browser_download_url': f'{base}/dl/{name}', 'size': len(payload)},
                       {'name': 'SHA256SUMS.txt', 'browser_download_url': f'{base}/dl/SHA256SUMS.txt', 'size': 100}]}


RELEASES = {'update': [rel('v0.3.1'), rel('v0.3.0')], 'bad': [rel('v0.3.1')], 'none': [rel('v0.3.0')],
            'pre': [rel('v0.4.0-beta.1', True), rel('v0.3.0')]}


class H(http.server.BaseHTTPRequestHandler):
    def send(self, code, body=b'', headers=()):
        self.send_response(code)
        for k, v in headers: self.send_header(k, v)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        p = self.path.split('?')[0]
        if scenario == 'error': return self.send(500)
        if scenario == 'limited': return self.send(403, b'{}', [('X-RateLimit-Remaining', '0'), ('X-RateLimit-Reset', str(int(time.time()) + 3600))])
        rels = RELEASES.get(scenario, RELEASES['update'])
        stable = [r for r in rels if not r['prerelease']]
        if p == '/repos/carbongotfound/reach/releases/latest':
            return self.send(200, json.dumps(stable[0]).encode(), [('ETag', f'"{scenario}"')]) if stable else self.send(404)
        if p == '/repos/carbongotfound/reach/releases':
            return self.send(200, json.dumps(rels).encode(), [('ETag', f'"{scenario}-all"')])
        if p == '/dl/SHA256SUMS.txt':
            h = '0' * 64 if scenario == 'bad' else hashlib.sha256(payload).hexdigest()
            return self.send(200, ''.join(f'{h}  {a["name"]}\n' for r in rels for a in r['assets'][:1]).encode())
        if p.startswith('/dl/Reach_'):
            return self.send(200, payload, [('Content-Type', 'application/octet-stream')])
        self.send(404)

    def log_message(self, *a): pass


print(f'mock releases ({scenario}) on {base}, pid {os.getpid()}', flush=True)
http.server.ThreadingHTTPServer(('127.0.0.1', port), H).serve_forever()
