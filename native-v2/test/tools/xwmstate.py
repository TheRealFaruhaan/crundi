#!/usr/bin/env python3
# A window's _NET_WM_STATE set to the atoms given, as a window manager would
# answer (none on Xvfb): the kid lock watchdog's test (test/electron/gaps.mjs).
# xprop cannot: it interns "A,B" as one atom's name.
#
#   python3 -I native-v2/test/tools/xwmstate.py <window id> _NET_WM_STATE_FULLSCREEN _NET_WM_STATE_FOCUSED
import ctypes, os, sys

if not os.environ.get('DISPLAY') or os.environ['DISPLAY'] == ':0':
    sys.exit('only on the run\'s virtual display (xvfb-run)')
x = ctypes.cdll.LoadLibrary('libX11.so.6')
x.XOpenDisplay.restype = ctypes.c_void_p
x.XOpenDisplay.argtypes = [ctypes.c_char_p]
x.XInternAtom.restype = ctypes.c_ulong
x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
x.XChangeProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
x.XCloseDisplay.argtypes = [ctypes.c_void_p]

d = x.XOpenDisplay(None)
if not d: sys.exit('no display')
win = int(sys.argv[1], 0)
atoms = (ctypes.c_ulong * len(sys.argv[2:]))(*[x.XInternAtom(d, n.encode(), 0) for n in sys.argv[2:]])
XA_ATOM, PROP_MODE_REPLACE = 4, 0
# Format 32: Xlib takes an array of longs.
x.XChangeProperty(d, win, x.XInternAtom(d, b'_NET_WM_STATE', 0), XA_ATOM, 32, PROP_MODE_REPLACE, ctypes.cast(atoms, ctypes.c_void_p), len(atoms))
x.XSync(d, 0)
x.XCloseDisplay(d)
