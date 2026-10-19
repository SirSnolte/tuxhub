"""Fenster-Wächter für eine xpra-Sitzung des Hubs.

Maximiert jedes neue Hauptfenster auf dem angegebenen X-Display, damit die
App den Bereich im Hub komplett ausfüllt. Dialoge (transient oder Typ DIALOG)
bleiben unangetastet. Endet, sobald das Display weg ist.

Aufruf: python3 xpra_watch.py :DISPLAY
"""

import os
import struct
import sys
import time

os.environ["DISPLAY"] = sys.argv[1]

from xpra.x11.bindings.display_source import init_display_source  # noqa: E402

init_display_source()
from xpra.x11.bindings.core import X11CoreBindings  # noqa: E402
from xpra.x11.bindings.window import X11WindowBindings  # noqa: E402

core = X11CoreBindings()
X11 = X11WindowBindings()
root = X11.get_root_xid()
SUBSTRUCTURE = (1 << 19) | (1 << 20)  # SubstructureNotify | SubstructureRedirect
VERT = core.get_xatom("_NET_WM_STATE_MAXIMIZED_VERT")
HORZ = core.get_xatom("_NET_WM_STATE_MAXIMIZED_HORZ")


def windows(prop_value):
    if not prop_value:
        return ()
    size = 8 if len(prop_value) % 8 == 0 else 4
    return struct.unpack(f"={len(prop_value) // size}{'Q' if size == 8 else 'I'}", prop_value)


def prop(wid, name, kind):
    try:
        return X11.XGetWindowProperty(wid, name, kind)
    except Exception:
        return None


def is_main_window(wid):
    if prop(wid, "WM_TRANSIENT_FOR", "WINDOW"):
        return False
    kind = windows(prop(wid, "_NET_WM_WINDOW_TYPE", "ATOM"))
    dialog_types = {
        core.get_xatom(t)
        for t in ("_NET_WM_WINDOW_TYPE_DIALOG", "_NET_WM_WINDOW_TYPE_UTILITY", "_NET_WM_WINDOW_TYPE_SPLASH")
    }
    return not (set(kind) & dialog_types)


done = set()
while True:
    try:
        current = set(windows(prop(root, "_NET_CLIENT_LIST", "WINDOW")))
        for wid in current - done:
            if is_main_window(wid):
                X11.sendClientMessage(root, wid, False, SUBSTRUCTURE, "_NET_WM_STATE", 1, VERT, HORZ, 1, 0)
            done.add(wid)
        done &= current
        core.XSync()
    except Exception as e:  # Display weg → Sitzung beendet
        print(f"Wächter beendet: {e}", file=sys.stderr)
        break
    time.sleep(0.5)
