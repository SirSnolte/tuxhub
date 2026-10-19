#!/usr/bin/env python3
"""Minimaler RPC-Prozess für tuxhub: eine JSON-Nachricht pro Zeile."""

import json
import sys

settings = {}
count = 0


def send(msg):
    print(json.dumps(msg), flush=True)


def status():
    send({"method": "status", "params": {"running": False, "count": count,
                                         "text": f"{settings.get('greeting', 'Hallo')}! Zähler: {count}"}})


for line in sys.stdin:
    msg = json.loads(line)
    method, params, mid = msg.get("method"), msg.get("params") or {}, msg.get("id")
    if method == "shutdown":
        break
    try:
        if method == "configure":
            settings = params
            result = True
        elif method == "action" and params.get("id") == "count":
            count += 1
            result = count
        elif method == "echo":
            result = params
        else:
            raise ValueError(f"unbekannte Methode {method}")
        if mid is not None:
            send({"id": mid, "result": result})
        status()
    except Exception as e:  # Fehler an den Hub zurückgeben
        if mid is not None:
            send({"id": mid, "error": {"message": str(e)}})
