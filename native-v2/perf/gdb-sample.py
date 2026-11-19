import gdb, os, signal, threading
gdb.execute("set pagination off"); gdb.execute("set print frame-arguments none"); gdb.execute("set confirm off")
gdb.execute("handle SIGUSR2 stop print nopass")
N = int(os.environ.get('SAMPLES', '150')); out = open(os.environ['SAMPLE_OUT'], 'w')
pid = gdb.selected_inferior().pid
for i in range(N):
    threading.Timer(0.012, lambda: os.kill(pid, signal.SIGUSR2)).start()
    try: out.write(gdb.execute("continue", to_string=True)[-300:])
    except Exception as e: out.write("cont err %s\n" % e)
    try:
        gdb.execute("thread 1", to_string=True)
        bt = gdb.execute("bt", to_string=True)
    except Exception as e:
        bt = str(e)
    out.write("=== %d\n%s\n" % (i, bt)); out.flush()
gdb.execute("detach")
