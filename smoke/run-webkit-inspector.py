#!/usr/bin/env python3
"""Production inspector capture, loopback synthetic secrets only; prints counts."""
import importlib.util, json, os, pathlib, re, shlex, socket, subprocess, tempfile, threading, time, urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
root=pathlib.Path(__file__).resolve().parent.parent;prefix=root/'prefix'
spec=importlib.util.spec_from_file_location('capture',root/'tools/webview_capture.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers();self.wfile.write(b'<!doctype html><input id="password" type="password" value="SYNTHETIC_FORM_SECRET">')
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length',0)))
        self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Set-Cookie','synthetic_session=SYNTHETIC_HTTPONLY_SECRET; HttpOnly; Path=/');self.end_headers();self.wfile.write(b'{"synthetic_response":"SYNTHETIC_BODY_SECRET"}')
os.umask(0o077)
with tempfile.TemporaryDirectory(prefix='clbb-inspector-test-') as d:
    t=pathlib.Path(d);server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
    src=t/'fixture.c';binary=t/'fixture'
    s=(root/'smoke/webkit-diagnostics.c').read_text().replace('atl_webview_diagnostics_install(view);atl_webview_diagnostics_install(view);','webkit_settings_set_enable_developer_extras(webkit_web_view_get_settings(view),TRUE);').replace('g_timeout_add_seconds(2,','g_timeout_add_seconds(18,')
    s=s.replace('webkit_web_view_load_html(view,html,"http://localhost/");',f'(void)html;webkit_web_view_load_uri(view,"http://127.0.0.1:{server.server_port}/");')
    src.write_text(s)
    flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','webkitgtk-6.0','json-glib-1.0'],text=True))
    subprocess.run(['cc','-std=c11','-O2',str(src),str(root/'src/atl/src/api-impl-jni/widgets/webview_diagnostics.c'),'-I'+str(root/'src/atl/src/api-impl-jni/widgets'),*flags,'-o',str(binary)],check=True)
    with socket.socket() as x:x.bind(('127.0.0.1',0));port=x.getsockname()[1]
    e=os.environ.copy()
    for n,k in [('runtime','XDG_RUNTIME_DIR'),('cache','XDG_CACHE_HOME'),('data','XDG_DATA_HOME')]:
        p=t/n;p.mkdir(mode=0o700);e[k]=str(p)
    e.update(WEBKIT_INSPECTOR_HTTP_SERVER=f'127.0.0.1:{port}',GTK_A11Y='none',GDK_BACKEND='x11',GTK_USE_PORTAL='0',GSK_RENDERER='cairo',CLBB_WEBKIT_PROCESS_DIR=str(prefix/'lib/webkitgtk-6.0'),WEBKIT_INJECTED_BUNDLE_PATH=str(prefix/'lib/webkitgtk-6.0/injected-bundle'),LD_PRELOAD=str(prefix/'lib/libclbb_webkit_launch_adapter.so'))
    child=subprocess.Popen(['timeout','24s','xvfb-run','-a','dbus-run-session','--',str(binary)],env=e,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    c=None;ws=None
    try:
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}));paths=[]
        for i in range(100):
            try:
                html=opener.open(f'http://127.0.0.1:{port}/',timeout=1).read().decode();paths=re.findall(r'/socket/\d+/\d+/WebPage',html)
                if paths:break
            except OSError:pass
            time.sleep(.15)
        if not paths:raise RuntimeError('Inspector target missing')
        c=module.Capture(t/'capture','wip');ws=module.Socket(('127.0.0.1',port),paths[-1]);c.attach(ws)
        began=time.monotonic();end=began+7;sent=False;snapshotted=False
        while time.monotonic()<end:
            try:
                message=ws.receive()
                if message:c.message(message)
            except socket.timeout:pass
            if c.stats['ready'] and time.monotonic()-began>1.5 and not sent:
                c.command('Runtime.evaluate',{'expression':'localStorage.synthetic="SYNTHETIC_STORAGE_SECRET";fetch("/capture",{method:"POST",body:"SYNTHETIC_POST_SECRET"})'},target=c.active);sent=True
            if sent and not snapshotted and c.stats['bodies'] and c.stats['cookie_snapshots']>=2:
                c.snapshot(c.active);snapshotted=True
        raw=(t/'capture/events.jsonl').read_text()
        bodies=b''.join(f.read_bytes() for f in (t/'capture/bodies').iterdir())
        checks=[c.stats['ready'],c.stats['requests']>=1,c.stats['bodies']>=1,
                all(value in raw for value in ['SYNTHETIC_FORM_SECRET','SYNTHETIC_HTTPONLY_SECRET','SYNTHETIC_STORAGE_SECRET','SYNTHETIC_POST_SECRET']),
                b'SYNTHETIC_BODY_SECRET' in bodies,
                all((f.stat().st_mode&0o777)==0o600 for f in (t/'capture').rglob('*') if f.is_file())]
        if not all(checks):
            events=[json.loads(x) for x in raw.splitlines()]
            errors=[x['data']['message'] for x in events if x['kind']=='protocol' and ('error' in x['data']['message'] or x['data']['message'].get('result',{}).get('wasThrown'))]
            print(json.dumps({'stats':c.stats,'errors':errors,'synthetic_found':{v:v in raw for v in ['SYNTHETIC_FORM_SECRET','SYNTHETIC_HTTPONLY_SECRET','SYNTHETIC_STORAGE_SECRET','SYNTHETIC_POST_SECRET']},'body_bytes':len(bodies),'body_prefix':bodies[:200].decode(errors='replace')}))
            raise RuntimeError('Full inspector capture FAIL: '+str(checks))
        print('WEB_INSPECTOR request/body/HttpOnly cookie/form/storage/private-file capture PASS')
    finally:
        if c:c.close()
        if ws:ws.close()
        server.shutdown();child.wait(timeout=25)
