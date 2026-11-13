#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Move only the selected owned X11 game window; never activate or move a cursor.
import ctypes as C,json,time,pathlib,sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'));from runtime_identity import runtime_executable_is_owned
meta=json.loads((ROOT/'logs/current-atl-pid.json').read_text());pid=meta['pid']
assert runtime_executable_is_owned(pid,ROOT),'Not the selected owned runtime'
fields=pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
assert fields[0]not in ('Z','X','x')and int(fields[19])==meta['pid_start_ticks'],'Stale runtime identity'
X=C.CDLL('libX11.so.6');D=C.c_void_p;W=C.c_ulong;I=C.c_int
X.XOpenDisplay.argtypes=[C.c_char_p];X.XOpenDisplay.restype=D
X.XDefaultRootWindow.argtypes=[D];X.XDefaultRootWindow.restype=W
X.XInternAtom.argtypes=[D,C.c_char_p,I];X.XInternAtom.restype=W
X.XGetWindowProperty.argtypes=[D,W,W,C.c_long,C.c_long,I,W,C.POINTER(W),C.POINTER(I),C.POINTER(W),C.POINTER(W),C.POINTER(D)];X.XFree.argtypes=[D]
class Data(C.Union):_fields_=[('l',C.c_long*5),('b',C.c_char*20)]
class Client(C.Structure):_fields_=[('type',I),('serial',W),('send_event',I),('display',D),('window',W),('message_type',W),('format',I),('data',Data)]
class Event(C.Union):_fields_=[('client',Client),('pad',C.c_long*24)]
X.XSendEvent.argtypes=[D,W,I,C.c_long,C.POINTER(Event)];X.XFlush.argtypes=[D];X.XCloseDisplay.argtypes=[D]
d=X.XOpenDisplay(b':1');assert d;r=X.XDefaultRootWindow(d)
def prop(w,name):
 a,f,n,rem,data=W(),I(),W(),W(),D();X.XGetWindowProperty(d,w,X.XInternAtom(d,name,0),0,4096,0,0,C.byref(a),C.byref(f),C.byref(n),C.byref(rem),C.byref(data));v=list(C.cast(data,C.POINTER(W))[:n.value])if data and f.value==32 else []
 if data:X.XFree(data)
 return v
before=prop(r,b'_NET_ACTIVE_WINDOW');windows=[w for w in prop(r,b'_NET_CLIENT_LIST_STACKING')if prop(w,b'_NET_WM_PID')==[pid]];assert windows
for w in windows:
 e=Event();e.client.type=33;e.client.send_event=1;e.client.display=d;e.client.window=w;e.client.message_type=X.XInternAtom(d,b'_NET_MOVERESIZE_WINDOW',0);e.client.format=32;e.client.data.l[0]=(2<<12)|(1<<8)|(1<<9)|(1<<10)|(1<<11)|1;e.client.data.l[1]=2220;e.client.data.l[2]=76;e.client.data.l[3]=1308;e.client.data.l[4]=749;X.XSendEvent(d,r,0,(1<<19)|(1<<20),C.byref(e))
X.XFlush(d);time.sleep(.1);assert before==prop(r,b'_NET_ACTIVE_WINDOW');X.XCloseDisplay(d);print(json.dumps({'pid':pid,'xy':[2220,76],'focus_unchanged':True}))
