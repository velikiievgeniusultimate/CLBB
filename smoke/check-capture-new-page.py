#!/usr/bin/env python3
"""Exercise real collector discovery/transport against two inspector pages."""
import base64
import hashlib
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

root = pathlib.Path(__file__).resolve().parents[1]
second = threading.Event()
stopped = threading.Event()

class Inspector(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    def log_message(self, *args): pass
    def send_message(self, message):
        data = json.dumps(message).encode()
        header = bytes([0x81, len(data)]) if len(data)<126 else b'\x81\x7e'+struct.pack('!H',len(data))
        self.wfile.write(header+data); self.wfile.flush()
    def exact(self, count):
        value = self.rfile.read(count)
        if len(value)!=count: raise EOFError()
        return value
    def do_GET(self):
        if self.path=='/':
            pages = ['/socket/1/1/WebPage']+(['/socket/1/2/WebPage'] if second.is_set() else [])
            body = ' '.join(pages).encode()
            self.send_response(200); self.send_header('Content-Length',str(len(body))); self.end_headers()
            self.wfile.write(body); return
        page = 'second' if '/2/' in self.path else 'first'
        key = self.headers['Sec-WebSocket-Key']+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11'
        self.send_response(101); self.send_header('Upgrade','websocket'); self.send_header('Connection','Upgrade')
        self.send_header('Sec-WebSocket-Accept',base64.b64encode(hashlib.sha1(key.encode()).digest()).decode()); self.end_headers()
        self.send_message({'method':'Target.targetCreated','params':{'targetInfo':{'targetId':page,'type':'page'}}})
        try:
            while not stopped.is_set():
                head = self.exact(2); size = head[1]&127
                if size==126: size = struct.unpack('!H',self.exact(2))[0]
                elif size==127: size = struct.unpack('!Q',self.exact(8))[0]
                mask = self.exact(4); data = self.exact(size)
                outer = json.loads(bytes(byte^mask[i%4] for i,byte in enumerate(data)))
                if outer['method']!='Target.sendMessageToTarget': continue
                inner = json.loads(outer['params']['message']); method = inner['method']; result = {}
                if method=='Runtime.evaluate':
                    expression = inner.get('params',{}).get('expression')
                    value = page if expression=='document.title' else json.dumps({'url':'http://localhost/'+page,'inputs':[]})
                    result = {'result':{'type':'string','value':value},'wasThrown':False}
                self.send_message({'id':outer['id'],'result':{}})
                self.send_message({'method':'Target.dispatchMessageFromTarget','params':{'targetId':page,
                                   'message':json.dumps({'id':inner['id'],'result':result})}})
        except (OSError,EOFError): pass

os.umask(0o077)
server = ThreadingHTTPServer(('127.0.0.1',0),Inspector)
threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='clbb-retarget-') as temporary:
    status = pathlib.Path(temporary)/'status.json'
    child = subprocess.Popen([sys.executable,str(root/'tools/webview_capture.py'),'--port',str(server.server_port),
                              '--status',str(status)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    session = None
    sent = False
    try:
        deadline = time.monotonic()+10
        while time.monotonic()<deadline:
            if status.exists():
                state = json.loads(status.read_text()); session = pathlib.Path(state['directory'])
                if state['ready'] and not second.is_set(): second.set()
                if state['connections']>=2 and state['ready'] and not sent:
                    command = {'method':'Runtime.evaluate','params':{'expression':'document.title','returnByValue':True}}
                    (session/'commands/new-page.json').write_text(json.dumps(command)); sent=True
                result = session/'command-results/new-page.json'
                if result.exists():
                    assert json.loads(result.read_text())['result']['result']['value']=='second'
                    assert 'newer_page_available' in (session/'events.jsonl').read_text()
                    print('CAPTURE_NEW_PAGE_OK old_page_retained=1 controls_reach_second=1'); break
            time.sleep(.05)
        else: raise RuntimeError('Collector did not follow new page')
    finally:
        if session: (session/'STOP').touch()
        child.terminate(); child.wait(timeout=3)
        stopped.set(); server.shutdown(); server.server_close()
