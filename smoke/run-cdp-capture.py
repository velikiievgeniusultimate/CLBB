#!/usr/bin/env python3
"""Synthetic loopback-only CDP capture regression for the Android collector."""
import base64,importlib.util,json,os,pathlib,subprocess,tempfile,threading,time,urllib.request
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
root=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('capture',root/'tools/webview_capture.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers();self.wfile.write(b'<!doctype html><input type="password" value="SYNTHETIC_FORM_SECRET">')
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length',0)));self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Set-Cookie','synthetic=SYNTHETIC_HTTPONLY_SECRET; HttpOnly; Path=/');self.end_headers();self.wfile.write(b'{"value":"SYNTHETIC_BODY_SECRET"}')
os.umask(0o077)
with tempfile.TemporaryDirectory(prefix='clbb-cdp-test-') as directory:
    t=pathlib.Path(directory);profile=t/'profile';profile.mkdir(mode=0o700)
    server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
    env={k:v for k,v in os.environ.items() if k.lower() not in {'http_proxy','https_proxy','all_proxy','ld_preload','ld_library_path'}}
    args=['/usr/lib/chromium/chromium','--headless=new','--remote-debugging-port=0','--user-data-dir='+str(profile),'--no-first-run','--no-default-browser-check','--disable-background-networking','--disable-component-update','--disable-sync','--disable-default-apps','about:blank']
    child=subprocess.Popen(args,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    capture=None;ws=None
    try:
        for _ in range(100):
            if (profile/'DevToolsActivePort').exists():break
            if child.poll() is not None:raise RuntimeError('Chromium exited')
            time.sleep(.1)
        port=int((profile/'DevToolsActivePort').read_text().splitlines()[0]);opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
        targets=json.loads(opener.open(f'http://127.0.0.1:{port}/json/list').read());target=next(x for x in targets if x['type']=='page');path=target['webSocketDebuggerUrl'].split(str(port),1)[1]
        capture=module.Capture(t/'capture','cdp');ws=module.Socket(('127.0.0.1',port),path);capture.attach(ws)
        capture.command('Page.navigate',{'url':f'http://127.0.0.1:{server.server_port}/'})
        (capture.directory/'commands/check.json').write_text(json.dumps({'method':'Runtime.evaluate','params':{'expression':'6*7','returnByValue':True}}))
        began=time.monotonic();sent=False;snap=False
        while time.monotonic()-began<5:
            capture.controls()
            try:
                message=ws.receive()
                if message:capture.message(message)
            except module.socket.timeout:pass
            if capture.stats['ready'] and time.monotonic()-began>1 and not sent:
                capture.command('Runtime.evaluate',{'expression':'localStorage.synthetic="SYNTHETIC_STORAGE_SECRET";fetch("/capture",{method:"POST",body:"SYNTHETIC_POST_SECRET"})'});sent=True
            if sent and time.monotonic()-began>2 and not snap:capture.snapshot(None);snap=True
        raw=(t/'capture/events.jsonl').read_text();body=b''.join(x.read_bytes() for x in (t/'capture/bodies').iterdir())
        streamed=b''.join(base64.b64decode(x['data']['base64']) for x in map(json.loads,raw.splitlines()) if x['kind'] in {'stream_buffer','stream_chunk'})
        control=json.loads((capture.directory/'command-results/check.json').read_text())
        checks=[control.get('result',{}).get('result',{}).get('value')==42,capture.stats['ready'],b'SYNTHETIC_BODY_SECRET' in streamed,all(v in raw for v in ['SYNTHETIC_FORM_SECRET','SYNTHETIC_STORAGE_SECRET','SYNTHETIC_HTTPONLY_SECRET','SYNTHETIC_POST_SECRET']),b'SYNTHETIC_BODY_SECRET' in body]
        if not all(checks):raise RuntimeError('CDP full capture FAIL '+str(checks))
        print('CDP request/body/HttpOnly cookie/form/storage capture PASS')
    finally:
        if capture:capture.close()
        if ws:ws.close()
        child.terminate()
        try:child.wait(timeout=5)
        except subprocess.TimeoutExpired:child.kill();child.wait()
        server.shutdown()
