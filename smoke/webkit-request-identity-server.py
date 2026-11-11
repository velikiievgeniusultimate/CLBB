#!/usr/bin/env python3
"""Loopback fixture only: never forwards requests or reads credentials."""
import http.server
import json
import sys

class Server(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        actual = self.headers.get('X-Requested-With')
        if self.path.startswith('/headers'):
            content = json.dumps({'header': actual}).encode()
            mime = 'application/json'
        elif self.path.startswith('/resource.js'):
            content = ('window.resourceIdentity=' + json.dumps(actual) + ';').encode()
            mime = 'application/javascript'
        else:
            expected = 'com.example.atl_identity' if self.path == '/valid' else None
            content = ('''<!doctype html><meta charset="utf-8"><title>Local request identity</title>
<script src="/resource.js"></script><script>
const mainActual=MAIN, expected=EXPECTED;
(async()=>{
 const results=[mainActual===expected,window.resourceIdentity===expected];
 const f=async headers=>(await (await fetch('/headers',{headers,cache:'no-store'})).json()).header;
 results.push(await f({})===expected);
 results.push(await f({'x-requested-with':'fixture-explicit'})==='fixture-explicit');
 results.push(await new Promise(resolve=>{const x=new XMLHttpRequest();x.open('GET','/headers');
 x.setRequestHeader('x-ReQuEsTeD-wItH','fixture-xhr');x.onload=()=>resolve(JSON.parse(x.responseText).header==='fixture-xhr');x.onerror=()=>resolve(false);x.send();}));
 results.push(await f({'X-Requested-With':''})==='');
 window.webkit.messageHandlers.test.postMessage(results.every(Boolean));
})().catch(()=>window.webkit.messageHandlers.test.postMessage(false));
</script>'''.replace('MAIN', json.dumps(actual)).replace('EXPECTED', json.dumps(expected))).encode()
            mime = 'text/html'
        self.send_response(200)
        self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(content)))
        self.send_header('Cache-Control', 'no-store')
        self.end_headers()
        self.wfile.write(content)

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Server)
with open(sys.argv[1], 'w') as output:
    output.write(str(server.server_port))
server.serve_forever()
