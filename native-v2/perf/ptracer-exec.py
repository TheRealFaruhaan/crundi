import ctypes, os, sys
libc = ctypes.CDLL(None)
PR_SET_PTRACER = 0x59616d61
libc.prctl(PR_SET_PTRACER, ctypes.c_ulong(-1 & 0xffffffffffffffff), 0, 0, 0)
bin = os.environ.get('NATIVE_BIN') or os.path.expanduser('~/.cache/crundi-native-target/release/crundi-v2')
os.execv(bin, [bin] + sys.argv[1:])
