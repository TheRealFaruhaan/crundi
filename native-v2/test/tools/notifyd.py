#!/usr/bin/env python3
# A notification service on the session bus (org.freedesktop.Notifications,
# as GNOME's or dunst's): each Notify call's arguments go to stdout as a JSON
# line; a line "click <id>" on stdin emits ActionInvoked(id, "default") as a
# click on the notification would (FINDINGS T9).
import json, sys, threading
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.Notifications">
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/>
<arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/>
<arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface></node>"""
PATH, IFACE = "/org/freedesktop/Notifications", "org.freedesktop.Notifications"
node = Gio.DBusNodeInfo.new_for_xml(XML)
state = {"next": 100, "conn": None}

def call(conn, sender, path, iface, method, params, inv):
    if method == "GetCapabilities":
        inv.return_value(GLib.Variant("(as)", (["actions", "body"],))); return
    app, rep, icon, summary, body, actions, hints, expire = params.unpack()
    state["next"] += 1
    hints = {k: (v if isinstance(v, (str, int, bool, float)) else str(v)) for k, v in hints.items()}
    print(json.dumps({"id": state["next"], "app": app, "replaces": rep, "icon": icon, "summary": summary, "body": body,
                      "actions": actions, "hints": hints, "expire": expire}), flush=True)
    inv.return_value(GLib.Variant("(u)", (state["next"],)))

def acquired(conn, name):
    state["conn"] = conn
    print(json.dumps({"ready": True}), flush=True)

def bus(conn, name):
    conn.register_object(PATH, node.interfaces[0], call, None, None)

loop = GLib.MainLoop()
Gio.bus_own_name(Gio.BusType.SESSION, IFACE, Gio.BusNameOwnerFlags.NONE, bus, acquired, lambda *a: loop.quit())

def stdin():
    for line in sys.stdin:
        w = line.split()
        if len(w) == 2 and w[0] == "click":
            GLib.idle_add(lambda i=int(w[1]): state["conn"].emit_signal(None, PATH, IFACE, "ActionInvoked", GLib.Variant("(us)", (i, "default"))) and False)
    GLib.idle_add(loop.quit)
threading.Thread(target=stdin, daemon=True).start()
loop.run()
