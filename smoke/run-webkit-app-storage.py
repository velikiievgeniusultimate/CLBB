#!/usr/bin/env python3
"""Real WebKit storage isolation/persistence with local synthetic cookies; no HTTP."""
import http.server,os,pathlib,shlex,subprocess,tempfile,threading
root=pathlib.Path(__file__).resolve().parent.parent;prefix=root/'prefix'
with tempfile.TemporaryDirectory(prefix='clbb-app-storage-') as directory:
    temp=pathlib.Path(directory);binary=temp/'storage'
    flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','webkitgtk-6.0'],text=True))
    subprocess.run(['cc','-std=c11','-O2','-Wall','-Wextra','-Werror',str(root/'smoke/webkit-app-storage.c'),
                    str(root/'src/atl/src/api-impl-jni/widgets/webview_session.c'),
                    '-I'+str(root/'src/atl/src/api-impl-jni/widgets'),*flags,'-o',str(binary)],check=True)
    for mode in ('write','read'):
        environment=os.environ.copy()
        for name,key in [('runtime','XDG_RUNTIME_DIR'),('cache','XDG_CACHE_HOME'),('host-data','XDG_DATA_HOME')]:
            path=temp/(name+'-'+mode);path.mkdir(mode=0o700);environment[key]=str(path)
        environment.update(GTK_A11Y='none',GDK_BACKEND='x11',GTK_USE_PORTAL='0',GSK_RENDERER='cairo',
            CLBB_WEBKIT_PROCESS_DIR=str(prefix/'lib/webkitgtk-6.0'),
            WEBKIT_INJECTED_BUNDLE_PATH=str(prefix/'lib/webkitgtk-6.0/injected-bundle'),
            LD_PRELOAD=str(prefix/'lib/libclbb_webkit_launch_adapter.so'))
        result=subprocess.run(['timeout','18s','xvfb-run','-a','dbus-run-session','--',str(binary),
                                str(temp/'app-a'),str(temp/'app-b'),mode],env=environment,capture_output=True,text=True)
        for line in result.stdout.splitlines():
            if line.startswith('APP_STORAGE'):print(line,flush=True)
        if result.returncode:raise SystemExit('App storage test failed: '+str(result.returncode))
    binary=temp/'storage-dom'
    subprocess.run(['cc','-std=c11','-O2','-Wall','-Wextra','-Werror',str(root/'smoke/webkit-app-storage-dom.c'),
                    str(root/'src/atl/src/api-impl-jni/widgets/webview_session.c'),
                    '-I'+str(root/'src/atl/src/api-impl-jni/widgets'),*flags,'-o',str(binary)],check=True)
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_GET(self):
            mode=self.path.strip('/')
            if mode not in ('write','read','empty'):self.send_error(404);return
            body=("<!doctype html><script>let v=localStorage.getItem('clbb_dom_fixture');let result;"
                  "if('MODE'==='write'){localStorage.setItem('clbb_dom_fixture','local_test_only');result=v===null?'WRITE_EMPTY':'BAD';}"
                  "else result=v===null?'EMPTY':v==='local_test_only'?'READ_OK':'BAD';"
                  "window.webkit.messageHandlers.storage.postMessage(result);</script>".replace('MODE',mode)).encode()
            self.send_response(200);self.send_header('Content-Type','text/html');self.send_header('Cache-Control','no-store')
            self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        for index,(app,mode,expected) in enumerate((('app-a','write','WRITE_EMPTY'),('app-b','empty','EMPTY'),('app-a','read','READ_OK'))):
            for name,key in [('runtime','XDG_RUNTIME_DIR'),('cache','XDG_CACHE_HOME'),('host-data','XDG_DATA_HOME')]:
                path=temp/(name+'-dom-'+str(index));path.mkdir(mode=0o700);environment[key]=str(path)
            result=subprocess.run(['timeout','18s','xvfb-run','-a','dbus-run-session','--',str(binary),
                                    str(temp/app),'http://127.0.0.1:'+str(server.server_port)+'/'+mode,expected],
                                    env=environment,capture_output=True,text=True)
            for line in result.stdout.splitlines():
                if line.startswith('APP_DOM'):print(mode,line,flush=True)
            if result.returncode:raise SystemExit('App DOM storage test failed: '+str(result.returncode))
    finally:
        server.shutdown();server.server_close();thread.join()
