#!/usr/bin/env python3
"""Reference application for the tuxhub live interface (protocol 1).

Uses only the Python standard library. The application shows a text in a
small window and offers its settings on the Unix socket
$XDG_RUNTIME_DIR/tuxhub/live-demo.sock: one JSON object per line, requests
with an "id", answers that repeat it, notifications without one. tuxhub
renders the settings as a native page; changes in either place show up in
the other one at once.

  live_demo.py              window and socket
  live_demo.py --headless   socket only, changes are printed

The window needs tkinter (Fedora: python3-tkinter). Without it the
application runs headless.
"""

import json
import os
import queue
import re
import signal
import socket
import stat
import sys
import threading

APP_ID = "live-demo"
NAME = "Live Demo"
VERSION = "0.1.0"
PROTOCOL = 1
MAX_LINE = 1 << 20

TEXTS = {
    "en": {
        "look": "Look",
        "text_section": "Text",
        "size": "Size",
        "color": "Text color",
        "background": "Background",
        "font": "Font",
        "font_sans": "Sans",
        "font_serif": "Serif",
        "font_mono": "Monospace",
        "padding": "Padding",
        "padding_help": "Space around the text.",
        "shadow": "Shadow",
        "text": "Text",
        "text_help": "Shown in the window. At most 40 characters.",
        "note": "Note",
        "note_help": "A second, smaller line. Line breaks are kept.",
        "flash": "Flash the window",
        "reset": "Reset",
        "reset_confirm": "Reset all settings of the Live Demo?",
        "empty": "The text must not be empty.",
        "window": "Live Demo",
    },
    "de": {
        "look": "Aussehen",
        "text_section": "Text",
        "size": "Größe",
        "color": "Textfarbe",
        "background": "Hintergrund",
        "font": "Schrift",
        "font_sans": "Serifenlos",
        "font_serif": "Serif",
        "font_mono": "Festbreite",
        "padding": "Abstand",
        "padding_help": "Platz um den Text.",
        "shadow": "Schatten",
        "text": "Text",
        "text_help": "Wird im Fenster angezeigt. Höchstens 40 Zeichen.",
        "note": "Notiz",
        "note_help": "Eine zweite, kleinere Zeile. Zeilenumbrüche bleiben erhalten.",
        "flash": "Fenster aufblinken lassen",
        "reset": "Zurücksetzen",
        "reset_confirm": "Alle Einstellungen der Live Demo zurücksetzen?",
        "empty": "Der Text darf nicht leer sein.",
        "window": "Live Demo",
    },
}

DEFAULTS = {
    "size": 48,
    "color": "#3584e4ff",
    "background": "#ffffff",
    "font": "sans",
    "padding": 24,
    "shadow": False,
    "text": "tuxhub",
    "note": "",
}

COLOR = re.compile(r"^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$")


def texts(locale):
    """Labels in the requested language, English otherwise."""
    return TEXTS.get(locale, TEXTS["en"])


def schema(locale):
    tx = texts(locale)
    return {
        "sections": [
            {
                "id": "look",
                "label": tx["look"],
                "fields": [
                    {"key": "size", "type": "number", "label": tx["size"], "min": 12, "max": 160,
                     "step": 1, "unit": "px", "slider": True, "default": DEFAULTS["size"]},
                    {"key": "color", "type": "color", "label": tx["color"], "alpha": True},
                    {"key": "background", "type": "color", "label": tx["background"]},
                    {"key": "font", "type": "select", "label": tx["font"], "options": [
                        {"value": "sans", "label": tx["font_sans"]},
                        {"value": "serif", "label": tx["font_serif"]},
                        {"value": "mono", "label": tx["font_mono"]},
                    ]},
                    {"key": "padding", "type": "number", "label": tx["padding"], "help": tx["padding_help"],
                     "min": 0, "max": 80, "step": 4, "unit": "px"},
                    {"key": "shadow", "type": "toggle", "label": tx["shadow"]},
                ],
            },
            {
                "id": "text",
                "label": tx["text_section"],
                "fields": [
                    {"key": "text", "type": "text", "label": tx["text"], "help": tx["text_help"],
                     "max_length": 40},
                    {"key": "note", "type": "text", "label": tx["note"], "help": tx["note_help"],
                     "multiline": True, "max_length": 200},
                ],
            },
        ],
        "actions": [
            {"id": "flash", "label": tx["flash"]},
            {"id": "reset", "label": tx["reset"], "danger": True, "confirm": tx["reset_confirm"]},
        ],
    }


class Refused(Exception):
    """A value the application does not accept; the message is shown at the field."""

    def __init__(self, message, field=None):
        super().__init__(message)
        self.field = field


def check(key, value, locale):
    """Checks a value from tuxhub. Never trust the client: check everything again."""
    field = next((f for s in schema(locale)["sections"] for f in s["fields"] if f["key"] == key), None)
    if field is None:
        raise Refused(f"unknown setting {key!r}")
    kind = field["type"]
    if kind == "toggle" and isinstance(value, bool):
        return value
    if kind == "number" and isinstance(value, (int, float)) and not isinstance(value, bool):
        if field["min"] <= value <= field["max"]:
            return int(value) if float(value).is_integer() else value
        raise Refused(f"{value} is not between {field['min']} and {field['max']}", key)
    if kind == "select" and value in [o["value"] for o in field["options"]]:
        return value
    if kind == "color" and isinstance(value, str) and COLOR.match(value):
        if len(value) == 9 and not field.get("alpha"):
            raise Refused("no transparency here", key)
        return value.lower()
    if kind == "text" and isinstance(value, str):
        if len(value) > field["max_length"]:
            raise Refused(f"at most {field['max_length']} characters", key)
        if key == "text" and not value.strip():
            raise Refused(texts(locale)["empty"], key)
        return value
    raise Refused(f"{value!r} does not fit {key}", key)


class App:
    """Settings and connected clients. The socket threads and the window share it."""

    def __init__(self):
        self.lock = threading.Lock()
        self.values = dict(DEFAULTS)
        self.clients = []  # Client
        # Changes for the window: (key, value), or "flash" / "quit".
        self.ui = queue.Queue()
        self.stopping = threading.Event()

    def get(self):
        with self.lock:
            return dict(self.values)

    def update(self, changes, origin=None):
        """Applies changes and tells every other client and the window."""
        with self.lock:
            changes = {k: v for k, v in changes.items() if self.values.get(k) != v}
            self.values.update(changes)
            others = [c for c in self.clients if c is not origin]
        if not changes:
            return
        print(f"changed: {json.dumps(changes, ensure_ascii=False)}", flush=True)
        for c in others:
            c.send({"method": "changed", "params": {"values": changes}})
        if origin is not None:
            self.ui.put(("values", changes))

    def quit(self):
        self.stopping.set()
        self.ui.put(("quit", None))


class Client:
    def __init__(self, app, conn):
        self.app = app
        self.conn = conn
        self.write_lock = threading.Lock()
        self.locale = "en"

    def send(self, msg):
        line = (json.dumps(msg, ensure_ascii=False) + "\n").encode()
        with self.write_lock:
            try:
                self.conn.sendall(line)
            except OSError:
                pass

    def serve(self):
        with self.app.lock:
            self.app.clients.append(self)
        try:
            reader = self.conn.makefile("rb")
            while True:
                line = reader.readline(MAX_LINE + 1)
                if not line or not line.endswith(b"\n"):
                    break  # closed, or a line that is too long
                try:
                    msg = json.loads(line)
                except ValueError:
                    break
                if isinstance(msg, dict) and "id" in msg:
                    self.answer(msg)
        except OSError:
            pass
        finally:
            with self.app.lock:
                self.app.clients.remove(self)
            self.conn.close()

    def answer(self, msg):
        mid, method, params = msg.get("id"), msg.get("method"), msg.get("params") or {}
        try:
            result = self.handle(method, params)
            self.send({"id": mid, "result": result})
        except Refused as e:
            error = {"message": str(e)}
            if e.field:
                error["field"] = e.field
            self.send({"id": mid, "error": error})
        if method == "quit":
            self.app.quit()

    def handle(self, method, params):
        if method == "hello":
            if params.get("locale") in TEXTS:
                self.locale = params["locale"]
            return {"protocol": PROTOCOL, "name": NAME, "version": VERSION}
        if method == "schema":
            if params.get("locale") in TEXTS:
                self.locale = params["locale"]
            return schema(self.locale)
        if method == "get":
            return {"values": self.app.get()}
        if method == "set":
            key = params.get("key")
            value = check(key, params.get("value"), self.locale)
            self.app.update({key: value}, origin=self)
            return {"value": value}
        if method == "action":
            if params.get("id") == "flash":
                self.app.ui.put(("flash", None))
                return True
            if params.get("id") == "reset":
                self.app.update(dict(DEFAULTS), origin=self)
                # The client that asked gets the new values too.
                self.send({"method": "changed", "params": {"values": self.app.get()}})
                return True
            raise Refused(f"unknown action {params.get('id')!r}")
        if method == "quit":
            return True
        raise Refused(f"unknown method {method!r}")


def socket_path():
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    if not runtime:
        sys.exit("XDG_RUNTIME_DIR is not set")
    folder = os.path.join(runtime, "tuxhub")
    os.makedirs(folder, mode=0o700, exist_ok=True)
    os.chmod(folder, 0o700)
    return os.path.join(folder, f"{APP_ID}.sock")


def listen(path):
    """Opens the socket. A socket file nobody listens on is replaced; a running
    instance is left alone."""
    if os.path.lexists(path):
        if not stat.S_ISSOCK(os.lstat(path).st_mode):
            sys.exit(f"{path} exists and is not a socket")
        probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            probe.connect(path)
            sys.exit(f"{NAME} is already running")
        except OSError:
            os.unlink(path)
        finally:
            probe.close()
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    old = os.umask(0o177)  # the socket is created with mode 0600
    try:
        server.bind(path)
    finally:
        os.umask(old)
    server.listen()
    return server


def accept(app, server):
    while not app.stopping.is_set():
        try:
            conn, _ = server.accept()
        except OSError:
            break
        threading.Thread(target=Client(app, conn).serve, daemon=True).start()


def run_window(app):
    import tkinter as tk

    tx = texts("de" if os.environ.get("LANG", "").startswith("de") else "en")
    root = tk.Tk()
    root.title(tx["window"])
    root.geometry("520x320")
    fonts = {"sans": "Sans", "serif": "Serif", "mono": "Monospace"}

    canvas = tk.Canvas(root, highlightthickness=0)
    canvas.pack(fill="both", expand=True)
    controls = tk.Frame(root)
    controls.pack(fill="x", padx=8, pady=8)
    size = tk.IntVar(value=app.get()["size"])
    shadow = tk.BooleanVar(value=app.get()["shadow"])
    text = tk.StringVar(value=app.get()["text"])

    def draw():
        v = app.get()
        canvas.delete("all")
        canvas.configure(bg=v["background"][:7])
        w, h = canvas.winfo_width(), canvas.winfo_height()
        font = (fonts[v["font"]], -int(v["size"]))
        x, y = w // 2, h // 2
        if v["shadow"]:
            canvas.create_text(x + 3, y + 3, text=v["text"], font=font, fill="#888888")
        canvas.create_text(x, y, text=v["text"], font=font, fill=v["color"][:7])
        if v["note"]:
            canvas.create_text(x, y + int(v["size"]) // 2 + 12 + v["padding"], text=v["note"],
                               font=(fonts[v["font"]], -14), fill=v["color"][:7], anchor="n")

    def from_window(key, value):
        # A change in the application itself: tuxhub gets `changed`.
        app.update({key: value})
        draw()

    tk.Label(controls, text=tx["size"]).pack(side="left")
    tk.Scale(controls, from_=12, to=160, orient="horizontal", variable=size, showvalue=True,
             command=lambda _: from_window("size", size.get())).pack(side="left", fill="x", expand=True)
    tk.Checkbutton(controls, text=tx["shadow"], variable=shadow,
                   command=lambda: from_window("shadow", shadow.get())).pack(side="left", padx=8)
    entry = tk.Entry(controls, textvariable=text, width=14)
    entry.pack(side="left")

    def commit_text(_event=None):
        value = text.get()[:40]
        if value.strip():
            from_window("text", value)
        else:
            text.set(app.get()["text"])

    entry.bind("<Return>", commit_text)
    entry.bind("<FocusOut>", commit_text)

    def poll():
        try:
            while True:
                kind, data = app.ui.get_nowait()
                if kind == "quit":
                    root.destroy()
                    return
                if kind == "flash":
                    for i, color in enumerate(["#f6d32d", None, "#f6d32d", None]):
                        root.after(150 * i, lambda c=color: canvas.configure(bg=c or app.get()["background"][:7]))
                if kind == "values":
                    size.set(app.get()["size"])
                    shadow.set(app.get()["shadow"])
                    if root.focus_get() is not entry:
                        text.set(app.get()["text"])
        except queue.Empty:
            pass
        draw()
        root.after(50, poll)

    canvas.bind("<Configure>", lambda _: draw())
    root.protocol("WM_DELETE_WINDOW", app.quit)
    root.after(50, poll)
    root.mainloop()


def main():
    headless = "--headless" in sys.argv
    app = App()
    path = socket_path()
    server = listen(path)
    print(f"{NAME} listens on {path}", flush=True)
    signal.signal(signal.SIGTERM, lambda *_: app.quit())
    signal.signal(signal.SIGINT, lambda *_: app.quit())
    threading.Thread(target=accept, args=(app, server), daemon=True).start()
    try:
        if not headless:
            try:
                run_window(app)
            except ImportError:
                print("tkinter is not installed: running without a window", flush=True)
                headless = True
            except Exception as e:  # no display
                print(f"no window ({e}): running without a window", flush=True)
                headless = True
        if headless:
            while not app.stopping.wait(0.2):
                pass
    finally:
        server.close()
        try:
            os.unlink(path)
        except FileNotFoundError:
            pass
        print(f"{NAME} quit", flush=True)


if __name__ == "__main__":
    main()
