#!/usr/bin/env python3
"""Opt-in full local capture of an owned WebView via its standard inspector.

Contains secrets: output goes outside the repository, private permissions only.
No requests are replayed and no page network functions are replaced.
"""
import argparse, base64, hashlib, json, os, pathlib, re, socket, struct, time, urllib.request, urllib.parse, uuid
from datetime import datetime, timezone

class Socket:
    def __init__(self, address, path, unix=False):
        self.s=socket.socket(socket.AF_UNIX if unix else socket.AF_INET,socket.SOCK_STREAM)
        self.s.settimeout(2);self.s.connect(('\0'+address) if unix else address)
        key=base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(('GET '+path+' HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: '+key+'\r\nSec-WebSocket-Version: 13\r\n\r\n').encode())
        self.buf=b'';self.fragment=b''
        while b'\r\n\r\n' not in self.buf:
            chunk=self.s.recv(4096)
            if not chunk:raise OSError('upgrade closed')
            self.buf+=chunk
            if len(self.buf)>16384:raise OSError('upgrade oversized')
        header,self.buf=self.buf.split(b'\r\n\r\n',1)
        expected=base64.b64encode(hashlib.sha1((key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest())
        if not header.startswith(b'HTTP/1.1 101') or expected.lower() not in header.lower():raise OSError('invalid upgrade')
        self.s.settimeout(.25)
    def exact(self,n):
        while len(self.buf)<n:
            chunk=self.s.recv(max(4096,n-len(self.buf)))
            if not chunk:raise OSError('closed')
            self.buf+=chunk
        data,self.buf=self.buf[:n],self.buf[n:];return data
    def send(self,payload,opcode=1):
        mask=os.urandom(4);n=len(payload)
        head=bytes([0x80|opcode,0x80|n]) if n<126 else bytes([0x80|opcode,0xfe])+struct.pack('!H',n) if n<65536 else bytes([0x80|opcode,0xff])+struct.pack('!Q',n)
        self.s.sendall(head+mask+bytes(v^mask[i%4] for i,v in enumerate(payload)))
    def receive(self):
        # Keep incomplete frames in the buffer across timeouts.
        offset=0
        def peek(n):
            while len(self.buf)<n:
                chunk=self.s.recv(max(4096,n-len(self.buf)))
                if not chunk:raise OSError('closed')
                self.buf+=chunk
            return self.buf[:n]
        h=peek(2);opcode=h[0]&15;n=h[1]&127;offset=2
        if n==126:n=struct.unpack('!H',peek(4)[2:4])[0];offset=4
        elif n==127:n=struct.unpack('!Q',peek(10)[2:10])[0];offset=10
        if n>32*1024*1024:raise OSError('frame oversized')
        mask=peek(offset+4)[offset:offset+4] if h[1]&128 else None
        if mask:offset+=4
        data=peek(offset+n)[offset:];self.buf=self.buf[offset+n:]
        if mask:data=bytes(v^mask[i%4] for i,v in enumerate(data))
        if opcode==8:raise OSError('target closed')
        if opcode==9:self.send(data,10);return None
        if opcode==10:return None
        if opcode in (0,1):
            self.fragment+=data
            if len(self.fragment)>32*1024*1024:raise OSError('message oversized')
            if h[0]&128:
                result=json.loads(self.fragment);self.fragment=b'';return result
        return None
    def close(self):self.s.close()

FORM_EXPRESSION='JSON.stringify({url:location.href,cookie:document.cookie,inputs:Array.from(document.querySelectorAll("input,textarea,select")).map(e=>({tag:e.tagName,type:e.type,name:e.name,id:e.id,value:e.value})),localStorage:Object.assign({},localStorage),sessionStorage:Object.assign({},sessionStorage)})'

class Capture:
    def __init__(self,directory,protocol,status_file=None):
        self.directory=pathlib.Path(directory);self.protocol=protocol;self.status_file=status_file
        self.directory.mkdir(mode=0o700,parents=True,exist_ok=False)
        (self.directory/'bodies').mkdir(mode=0o700)
        (self.directory/'commands').mkdir(mode=0o700)
        (self.directory/'command-results').mkdir(mode=0o700)
        self.file=(self.directory/'events.jsonl').open('x');os.chmod(self.file.name,0o600)
        self.sequence=0;self.pending={};self.targets=set();self.active=None;self.enabled=set();self.stats={'connections':0,'requests':0,'responses':0,'bodies':0,'post_bodies':0,'cookie_snapshots':0,'form_snapshots':0,'http_statuses':{},'ready':False}
        self.started=time.monotonic();self.bytes=0;self.ws=None;self.last_snapshot=0;self.streaming=set()
        self.record('capture_started',{'protocol':protocol,'consent':'user-explicit-full-local','started_utc':datetime.now(timezone.utc).isoformat()})
        self.status()
    def record(self,kind,data):
        line=json.dumps({'utc':datetime.now(timezone.utc).isoformat(),'monotonic_ns':time.monotonic_ns(),'kind':kind,'data':data},ensure_ascii=False)+'\n'
        self.file.write(line);self.file.flush();self.bytes+=len(line.encode())
    def status(self):
        if not self.status_file:return
        p=pathlib.Path(self.status_file);tmp=p.with_suffix('.tmp')
        tmp.write_text(json.dumps({'directory':str(self.directory),**self.stats,'recorded_bytes':self.bytes},indent=2));os.chmod(tmp,0o600);tmp.replace(p)
    def command(self,method,params=None,target=None,tag=None):
        self.sequence+=1;i=self.sequence;message={'id':i,'method':method,'params':params or {}}
        self.pending[i]=(method,tag,target)
        self.record('command',{'target':target,'message':message})
        if target:
            self.sequence+=1
            message={'id':self.sequence,'method':'Target.sendMessageToTarget','params':{'targetId':target,'message':json.dumps(message)}}
        self.ws.send(json.dumps(message).encode());return i
    def enable(self,target):
        if target in self.enabled:return
        self.enabled.add(target);self.active=target
        self.command('Network.enable',{'maxTotalBufferSize':64*1024*1024,'maxResourceBufferSize':16*1024*1024,'maxPostDataSize':4*1024*1024} if self.protocol=='cdp' else {},target,tag='enable')
        self.command('Page.enable',target=target)
        self.command('Runtime.enable',target=target)
        self.command('Page.getResourceTree' if self.protocol=='wip' else 'Page.getFrameTree',target=target)
        self.snapshot(target)
    def snapshot(self,target):
        self.command('Page.getCookies' if self.protocol=='wip' else 'Network.getAllCookies',target=target,tag='cookies')
        self.command('Runtime.evaluate',{'expression':FORM_EXPRESSION,'returnByValue':True},target=target,tag='form')
    def controls(self):
        if not self.enabled:return
        for path in sorted((self.directory/'commands').glob('*.json')):
            try:
                if path.stat().st_size>65536:raise ValueError('command oversized')
                command=json.loads(path.read_text())
                if command.get('method') not in {'Runtime.evaluate','Page.navigate','Page.reload'}:raise ValueError('unsupported control')
                self.command(command['method'],command.get('params',{}),self.active,tag=('control',path.name))
            except (ValueError,KeyError,OSError) as error:self.record('control_error',{'file':path.name,'class':type(error).__name__})
            finally:path.unlink(missing_ok=True)
    def body(self,rid,target):self.command('Network.getResponseBody',{'requestId':rid},target,tag=('body',rid))
    def message(self,message,target=None):
        self.record('protocol',{'target':target,'message':message})
        method=message.get('method');p=message.get('params',{})
        if method=='Target.targetCreated':
            info=p.get('targetInfo',{});tid=info.get('targetId');self.targets.add(tid)
            if info.get('type')=='page':self.enable(tid)
            return
        if method=='Target.dispatchMessageFromTarget':
            self.message(json.loads(p['message']),p['targetId']);return
        if method=='Target.targetDestroyed':self.enabled.discard(p.get('targetId'));return
        if method=='Network.requestWillBeSent':
            self.stats['requests']+=1;r=p.get('request',{})
            if self.protocol=='cdp' and r.get('hasPostData') and 'postData' not in r:
                self.command('Network.getRequestPostData',{'requestId':p['requestId']},target,tag='post')
        if method=='Network.dataReceived' and p.get('data') and (target,p.get('requestId')) in self.streaming:
            self.record('stream_chunk',{'target':target,'requestId':p['requestId'],'base64':p['data']})
        if method=='Network.responseReceived':
            self.stats['responses']+=1;r=p.get('response',{});code=str(r.get('status',r.get('statusCode','unknown')))
            self.stats['http_statuses'][code]=self.stats['http_statuses'].get(code,0)+1
            if self.protocol=='cdp' and urllib.parse.urlsplit(r.get('url','')).hostname in {'accountmtapi.mobilelegends.com','mtpassapi.mobilelegends.com','mtacc.mobilelegends.com','127.0.0.1','localhost'}:
                self.streaming.add((target,p['requestId']))
                self.command('Network.streamResourceContent',{'requestId':p['requestId']},target,tag=('stream',p['requestId']))
            if code.isdigit() and int(code)>=400:self.body(p['requestId'],target)
        if method=='Network.loadingFinished':
            self.body(p['requestId'],target)
            self.command('Page.getCookies' if self.protocol=='wip' else 'Network.getAllCookies',target=target,tag='cookies')
        if 'id' in message and message['id'] in self.pending:
            method,tag,origin=self.pending.pop(message['id']);result=message.get('result',{})
            if isinstance(tag,tuple) and tag[0]=='control':
                path=self.directory/'command-results'/tag[1]
                path.write_text(json.dumps(message,ensure_ascii=False));os.chmod(path,0o600)
                self.record('control_result',{'file':tag[1],'error':'error' in message})
            if 'error' not in message:
                if tag=='enable':self.stats['ready']=True;self.record('network_ready',{'target':origin})
                elif tag=='cookies':self.stats['cookie_snapshots']+=1
                elif tag=='form':self.stats['form_snapshots']+=1
                elif tag=='post':self.stats['post_bodies']+=1
                elif isinstance(tag,tuple) and tag[0]=='stream':
                    self.record('stream_buffer',{'target':origin,'requestId':tag[1],'base64':result.get('bufferedData','')})
                elif isinstance(tag,tuple) and tag[0]=='body':
                    body=result.get('body','');data=base64.b64decode(body) if result.get('base64Encoded') else body.encode()
                    name=f'{self.stats["bodies"]:06d}.bin';f=self.directory/'bodies'/name
                    with f.open('xb') as out:out.write(data)
                    os.chmod(f,0o600);self.bytes+=len(data);self.stats['bodies']+=1
                    self.record('body_saved',{'target':origin,'requestId':tag[1],'file':'bodies/'+name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
        self.status()
    def attach(self,ws):
        self.ws=ws;self.pending.clear();self.enabled.clear();self.targets.clear();self.streaming.clear();self.stats['connections']+=1
        self.record('attached',{});self.stats['ready']=False
        if self.protocol=='cdp':self.enable(None)
    def pump(self,reconnect_check=None):
        last_check=0
        while time.monotonic()-self.started<1800 and self.bytes<256*1024*1024 and not (self.directory/'STOP').exists():
            if reconnect_check and time.monotonic()-last_check>1:
                last_check=time.monotonic()
                if reconnect_check():
                    self.record('newer_page_available',{})
                    return True
            self.controls()
            try:
                m=self.ws.receive()
                if m:self.message(m)
            except socket.timeout:pass
            if time.monotonic()-self.last_snapshot>2 and self.enabled:
                self.snapshot(self.active);self.last_snapshot=time.monotonic()
        return False
    def close(self):
        self.record('capture_stopped',self.stats);self.stats['ready']=False;self.status();self.file.close()

def private_directory(label):
    root=pathlib.Path.home()/'.local/state/clbb/private-diagnostics';root.mkdir(mode=0o700,parents=True,exist_ok=True);os.chmod(root,0o700)
    return root/(datetime.now().strftime('%Y%m%d-%H%M%S')+'-'+label+'-'+uuid.uuid4().hex[:8])

def main():
    os.umask(0o077)
    parser=argparse.ArgumentParser();parser.add_argument('--port',type=int,required=True);parser.add_argument('--status',required=True);a=parser.parse_args()
    capture=Capture(private_directory('atl'),'wip',a.status)
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}));seen=None
    try:
        while time.monotonic()-capture.started<1800 and not (capture.directory/'STOP').exists():
            try:
                html=opener.open(f'http://127.0.0.1:{a.port}/',timeout=1).read().decode()
                paths=re.findall(r'/socket/\d+/\d+/WebPage',html)
                if not paths:time.sleep(.5);continue
                selected=paths[-1]
                def newer_page():
                    try:
                        current=opener.open(f'http://127.0.0.1:{a.port}/',timeout=1).read().decode()
                        available=re.findall(r'/socket/\d+/\d+/WebPage',current)
                        return bool(available) and available[-1]!=selected
                    except (OSError,ValueError):return False
                ws=Socket(('127.0.0.1',a.port),selected);capture.attach(ws)
                try:
                    if not capture.pump(newer_page):break
                finally:ws.close()
            except (OSError,ValueError,KeyError) as error:
                capture.record('connection_event',{'class':type(error).__name__});capture.stats['ready']=False;capture.status();time.sleep(.5)
    finally:capture.close()
if __name__=='__main__':main()
