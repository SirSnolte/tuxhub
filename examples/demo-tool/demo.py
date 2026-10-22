#!/usr/bin/env python3
"""Minimal RPC process for tuxhub: one JSON message per line on stdin/stdout."""

import json
import sys

settings = {}
count = 0


def send(msg):
    print(json.dumps(msg), flush=True)


def status():
    greeting = settings.get("greeting", "Hello")
    send({"method": "status", "params": {"running": False, "count": count, "text": f"{greeting}. Count: {count}"}})


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
            raise ValueError(f"unknown method {method}")
        if mid is not None:
            send({"id": mid, "result": result})
        status()
    except Exception as e:  # report errors back to tuxhub
        if mid is not None:
            send({"id": mid, "error": {"message": str(e)}})
