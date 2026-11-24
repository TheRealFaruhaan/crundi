#!/usr/bin/env python3
# Real key presses on the X display ($DISPLAY, the run's Xvfb), through
# XTEST, as a keyboard's: the system-wide shortcuts' test (test/electron/
# lifecycle.mjs). With --steal-focus a window of this script's own takes the
# keyboard focus first, so the app's window does not have it.
#
#   python3 -I native-v2/test/tools/xkeys.py ctrl+shift+l [--steal-focus]
import ctypes, os, sys, time

if not os.environ.get('DISPLAY') or os.environ['DISPLAY'] == ':0':
    sys.exit('only on the run\'s virtual display (xvfb-run)')
x = ctypes.cdll.LoadLibrary('libX11.so.6')
xt = ctypes.cdll.LoadLibrary('libXtst.so.6')
x.XOpenDisplay.restype = ctypes.c_void_p
x.XOpenDisplay.argtypes = [ctypes.c_char_p]
x.XStringToKeysym.restype = ctypes.c_ulong
x.XStringToKeysym.argtypes = [ctypes.c_char_p]
x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x.XDefaultRootWindow.restype = ctypes.c_ulong
x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x.XCreateSimpleWindow.restype = ctypes.c_ulong
x.XCreateSimpleWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_ulong]
x.XMapRaised.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
x.XGetInputFocus.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int)]
x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
xt.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]

NAMES = {'ctrl': 'Control_L', 'shift': 'Shift_L', 'super': 'Super_L', 'alt': 'Alt_L'}
combo = sys.argv[1].split('+')
d = x.XOpenDisplay(None)
if not d: sys.exit('no display')
if '--steal-focus' in sys.argv:
    w = x.XCreateSimpleWindow(d, x.XDefaultRootWindow(d), 0, 0, 200, 100, 0, 0, 0)
    x.XMapRaised(d, w)
    x.XSync(d, 0)
    time.sleep(0.3)
    x.XSetInputFocus(d, w, 1, 0)
    x.XSync(d, 0)
    time.sleep(0.2)
    f = ctypes.c_ulong(); r = ctypes.c_int()
    x.XGetInputFocus(d, ctypes.byref(f), ctypes.byref(r))
    print('focus', 'mine' if f.value == w else hex(f.value))
codes = []
for k in combo:
    ks = x.XStringToKeysym(NAMES.get(k.lower(), k).encode())
    c = x.XKeysymToKeycode(d, ks)
    if not c: sys.exit('no keycode for ' + k)
    codes.append(c)
for c in codes: xt.XTestFakeKeyEvent(d, c, 1, 0); x.XSync(d, 0); time.sleep(0.03)
for c in reversed(codes): xt.XTestFakeKeyEvent(d, c, 0, 0); x.XSync(d, 0); time.sleep(0.03)
x.XSync(d, 0)
time.sleep(0.5)
print('pressed', sys.argv[1])
