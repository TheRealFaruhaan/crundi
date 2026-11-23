#!/usr/bin/env python3
# What a window asks of the window manager (none on Xvfb): the ClientMessages
# sent to the root window (WM_CHANGE_STATE to minimise, _NET_WM_STATE to
# maximise), one JSON line each, for <seconds>: the window test
# (test/electron/window.mjs). It listens as any client may
# (SubstructureNotify on the root), redirecting nothing.
#
#   python3 -I native-v2/test/tools/xwmspy.py <seconds>
import ctypes, json, os, sys, time

if not os.environ.get('DISPLAY') or os.environ['DISPLAY'] == ':0':
    sys.exit('only on the run\'s virtual display (xvfb-run)')
x = ctypes.cdll.LoadLibrary('libX11.so.6')
x.XOpenDisplay.restype = ctypes.c_void_p
x.XOpenDisplay.argtypes = [ctypes.c_char_p]
x.XDefaultRootWindow.restype = ctypes.c_ulong
x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x.XSelectInput.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_long]
x.XPending.argtypes = [ctypes.c_void_p]
x.XNextEvent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
x.XGetAtomName.restype = ctypes.c_void_p
x.XGetAtomName.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x.XFree.argtypes = [ctypes.c_void_p]
x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]

d = x.XOpenDisplay(None)
if not d: sys.exit('no display')
SUBSTRUCTURE_NOTIFY, CLIENT_MESSAGE = 1 << 19, 33
x.XSelectInput(d, x.XDefaultRootWindow(d), SUBSTRUCTURE_NOTIFY)
x.XSync(d, 0)
print(json.dumps({'ready': True}), flush=True)


def name(a):
    if not a: return 0
    p = x.XGetAtomName(d, a)
    if not p: return a
    s = ctypes.string_at(p).decode()
    x.XFree(p)
    return s


# XClientMessageEvent on 64 bits: type 0, window 32, message_type 40, format 48, data.l 56.
ev = (ctypes.c_char * 192)()
end = time.time() + float(sys.argv[1] if len(sys.argv) > 1 else 10)
while time.time() < end:
    while x.XPending(d):
        x.XNextEvent(d, ev)
        raw = bytes(ev)
        if int.from_bytes(raw[0:4], 'little') != CLIENT_MESSAGE: continue
        u = lambda o: int.from_bytes(raw[o:o + 8], 'little')
        t = name(u(40))
        data = [u(56 + 8 * i) for i in range(5)]
        if t == '_NET_WM_STATE': data = [data[0], name(data[1]), name(data[2])]
        print(json.dumps({'window': u(32), 'type': t, 'data': data}), flush=True)
    time.sleep(0.02)
