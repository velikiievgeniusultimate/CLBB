#!/usr/bin/env python3
"""Read pixels from a verified ATL X11 window without focus or pointer changes."""
import ctypes as C
import ctypes.util
import argparse
import json
import pathlib
import struct
import time
import zlib

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--pid', type=int, help='Exact owned runtime PID; defaults to current-atl-pid.json')
parser.add_argument('--output', type=pathlib.Path, help='PNG destination; defaults to logs/owned-window-current.png')
parser.add_argument('--display', required=True, help='Exact selected local X11 display')
options = parser.parse_args()
import re
if not re.fullmatch(r':[0-9]{1,5}(?:\.[0-9]{1,3})?', options.display):
    raise SystemExit('Explicit local X11 display required')
pid = options.pid or json.loads((root / 'logs/current-atl-pid.json').read_text())['pid']
if pathlib.Path('/proc', str(pid), 'exe').resolve() != root / 'prefix/bin/android-translation-layer':
    raise SystemExit('Runtime identity mismatch')
X = C.CDLL(ctypes.util.find_library('X11'))
D, W, I, U = C.c_void_p, C.c_ulong, C.c_int, C.c_uint
X.XOpenDisplay.argtypes = [C.c_char_p]; X.XOpenDisplay.restype = D
X.XDefaultRootWindow.argtypes = [D]; X.XDefaultRootWindow.restype = W
X.XInternAtom.argtypes = [D, C.c_char_p, I]; X.XInternAtom.restype = W
X.XGetWindowProperty.argtypes = [D,W,W,C.c_long,C.c_long,I,W,C.POINTER(W),C.POINTER(I),C.POINTER(W),C.POINTER(W),C.POINTER(D)]
X.XFree.argtypes = [D]
X.XGetGeometry.argtypes = [D,W,C.POINTER(W),C.POINTER(I),C.POINTER(I),C.POINTER(U),C.POINTER(U),C.POINTER(U),C.POINTER(U)]
X.XSync.argtypes = [D,I]; X.XCloseDisplay.argtypes = [D]
error = []
@C.CFUNCTYPE(I,D,D)
def on_error(display, event):
    error.append(True)
    return 0
X.XSetErrorHandler.argtypes = [C.c_void_p]; X.XSetErrorHandler(on_error)
display = X.XOpenDisplay(options.display.encode('ascii'))
if not display: raise SystemExit('X11 display unavailable')
def property_values(window, name):
    actual,fmt,n,remaining,data = W(),I(),W(),W(),D()
    status = X.XGetWindowProperty(display,window,X.XInternAtom(display,name,0),0,4096,0,0,C.byref(actual),C.byref(fmt),C.byref(n),C.byref(remaining),C.byref(data))
    values = list(C.cast(data,C.POINTER(W))[:n.value]) if status == 0 and fmt.value == 32 and data else []
    if data: X.XFree(data)
    return values
class XImage(C.Structure):
    _fields_ = [('width',I),('height',I),('xoffset',I),('format',I),('data',D),
                ('byte_order',I),('bitmap_unit',I),('bitmap_bit_order',I),('bitmap_pad',I),
                ('depth',I),('bytes_per_line',I),('bits_per_pixel',I),
                ('red_mask',W),('green_mask',W),('blue_mask',W)]
X.XGetImage.argtypes = [D,W,I,I,U,U,W,I]; X.XGetImage.restype = C.POINTER(XImage)
X.XDestroyImage.argtypes = [C.POINTER(XImage)]
try:
    for window in reversed(property_values(X.XDefaultRootWindow(display), b'_NET_CLIENT_LIST_STACKING')):
        if property_values(window,b'_NET_WM_PID') != [pid]: continue
        parent,x,y,width,height,border,depth = W(),I(),I(),U(),U(),U(),U()
        if not X.XGetGeometry(display,window,C.byref(parent),C.byref(x),C.byref(y),C.byref(width),C.byref(height),C.byref(border),C.byref(depth)): continue
        if width.value < 600 or height.value < 300: continue
        error.clear()
        observed_ns = time.monotonic_ns()
        pixels = X.XGetImage(display,window,0,0,width.value,height.value,W(-1).value,2)
        X.XSync(display,0)
        read_completed_ns = time.monotonic_ns()
        if error or not pixels: continue
        try:
            info = pixels.contents
            if info.bits_per_pixel != 32 or info.byte_order != 0 or (info.red_mask,info.green_mask,info.blue_mask) != (0xff0000,0xff00,0xff):
                raise SystemExit('Unsupported X11 pixel layout')
            source = C.string_at(info.data,info.bytes_per_line*info.height)
            rows = bytearray()
            for y in range(info.height):
                row = source[y*info.bytes_per_line:y*info.bytes_per_line+info.width*4]
                rgb = bytearray(info.width*3)
                rgb[0::3],rgb[1::3],rgb[2::3] = row[2::4],row[1::4],row[0::4]
                rows.append(0)
                rows.extend(rgb)
            def chunk(kind, payload):
                return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload))
            output = options.output or root / 'logs/owned-window-current.png'
            output.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',info.width,info.height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b''))
            print(json.dumps({'pid':pid,'window':hex(window),'path':str(output),
                              'window_observed_ns':observed_ns,'pixels_read_ns':read_completed_ns,
                              'completed_ns':time.monotonic_ns()}))
            break
        finally: X.XDestroyImage(pixels)
    else: raise SystemExit('No readable owned game window')
finally: X.XCloseDisplay(display)
