import gdb, os, time, threading, signal
gdb.execute("set pagination off"); gdb.execute("set confirm off")
gdb.execute("handle SIGUSR2 stop print nopass")
out = open(os.environ['SAMPLE_OUT'], 'w'); t0 = time.time()
class BP(gdb.Breakpoint):
    def stop(self):
        try:
            bt = gdb.execute("bt 40", to_string=True)
        except Exception as e: bt = str(e)
        out.write("HIT %.2f %s\n%s\n" % (time.time() - t0, self.location, bt)); out.flush()
        return False
for loc in ["crundi_planning::kanban::load", "crundi_planning::mindmap::load_from"]:
    try: BP(loc)
    except Exception as e: out.write("bp err %s %s\n" % (loc, e))
pid = gdb.selected_inferior().pid
threading.Timer(float(os.environ.get('BP_SECONDS', '20')), lambda: os.kill(pid, signal.SIGUSR2)).start()
try: gdb.execute("continue")
except Exception as e: out.write("cont err %s\n" % e)
out.write("END %.2f\n" % (time.time() - t0)); out.flush()
gdb.execute("delete"); gdb.execute("detach")
