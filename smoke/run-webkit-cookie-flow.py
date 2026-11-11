#!/usr/bin/env python3
"""Local HTTPS only; synthetic cookies, actual production GTK policy adapter."""
import http.server, json, os, pathlib, shlex, ssl, subprocess, tempfile, threading

root=pathlib.Path(__file__).resolve().parent.parent
prefix=root/'prefix'
with tempfile.TemporaryDirectory(prefix='clbb-cookie-flow-') as directory:
    temp=pathlib.Path(directory)
    binary=temp/'cookie-flow'
    flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','webkitgtk-6.0'],text=True))
    subprocess.run(['cc','-std=c11','-O2','-Wall','-Wextra','-Werror',str(root/'smoke/webkit-cookie-flow.c'),
                    str(root/'src/atl/src/api-impl-jni/widgets/webview_cookie_policy.c'),
                    '-I'+str(root/'src/atl/src/api-impl-jni/widgets'),*flags,'-o',str(binary)],check=True)
    certificate=temp/'cert.pem';key=temp/'key.pem'
    subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost',
                    '-addext','subjectAltName=DNS:localhost,IP:127.0.0.1','-keyout',str(key),'-out',str(certificate)],
                    check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_GET(self):
            cookie=None
            if self.path=='/':
                body=('''<!doctype html><script>addEventListener('message',e=>{
if(e.origin==='https://127.0.0.1:PORT'&&/^CLBB_COOKIE_[01]$/.test(e.data))
window.webkit.messageHandlers.cookie.postMessage(e.data);});</script>
<iframe src="https://127.0.0.1:PORT/set"></iframe>'''.replace('PORT',str(self.server.server_port))).encode()
                mime='text/html'
            elif self.path=='/set':
                body=b"<!doctype html><script>fetch('/check').then(r=>r.json()).then(x=>parent.postMessage('CLBB_COOKIE_'+x.present,'*'));</script>"
                mime='text/html';cookie='clbb_fixture=local_only; Secure; HttpOnly; SameSite=None; Path=/'
            elif self.path=='/check':
                body=json.dumps({'present':int('clbb_fixture=local_only' in self.headers.get('Cookie',''))}).encode();mime='application/json'
            else:
                self.send_error(404);return
            self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Content-Length',str(len(body)))
            if cookie:self.send_header('Set-Cookie',cookie)
            self.end_headers();self.wfile.write(body)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(certificate,key)
    server.socket=context.wrap_socket(server.socket,server_side=True)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        for index,mode in enumerate(('deny','allow','deny')):
            environment=os.environ.copy()
            for k in list(environment):
                if k.lower() in ('http_proxy','https_proxy','all_proxy','no_proxy'):environment.pop(k,None)
            for name,var in [('runtime','XDG_RUNTIME_DIR'),('cache','XDG_CACHE_HOME'),('data','XDG_DATA_HOME')]:
                path=temp/(name+str(index));path.mkdir(mode=0o700);environment[var]=str(path)
            environment.update(GTK_A11Y='none',GTK_USE_PORTAL='0',GDK_BACKEND='x11',GSK_RENDERER='cairo',
                CLBB_WEBKIT_PROCESS_DIR=str(prefix/'lib/webkitgtk-6.0'),
                WEBKIT_INJECTED_BUNDLE_PATH=str(prefix/'lib/webkitgtk-6.0/injected-bundle'),
                LD_PRELOAD=str(prefix/'lib/libclbb_webkit_launch_adapter.so'))
            result=subprocess.run(['timeout','20s','xvfb-run','-a','dbus-run-session','--',str(binary),
                                    'https://localhost:'+str(server.server_port)+'/',str(certificate),mode],
                                    env=environment,capture_output=True,text=True)
            for line in result.stdout.splitlines():
                if line.startswith('COOKIE_'):print(line,flush=True)
            if result.returncode:raise SystemExit('Local cookie flow failed: '+str(result.returncode))
    finally:
        server.shutdown();server.server_close();thread.join()
