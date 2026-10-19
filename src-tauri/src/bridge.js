// tuxhub-Bridge für Tool-Oberflächen.
//
// In der eigenen HTML-Seite einbinden:
//   <script src="/__tuxhub/bridge.js"></script>
//
// Danach steht `window.tuxhub` bereit:
//   await tuxhub.call("methode", { ... })   // JSON-RPC an den rpc-Prozess des Tools
//   await tuxhub.action("toggle")           // eine Aktion aus tuxhub.toml auslösen
//   await tuxhub.getSettings()              // gespeicherte Einstellungen
//   await tuxhub.saveSettings({ key: 1 })   // speichern + an den Prozess schicken
//   await tuxhub.launch()                   // Tool normal starten
//   tuxhub.on("status", params => …)        // Benachrichtigungen des Prozesses
//   tuxhub.on("theme", ({ dark }) => …)     // Hell/Dunkel des Hubs
(() => {
  if (window.tuxhub) return;
  const pending = new Map();
  const listeners = new Map();
  let next = 0;

  window.addEventListener("message", (ev) => {
    if (ev.source !== window.parent) return;
    const m = ev.data;
    if (!m || m.tuxhub !== 1) return;
    if (m.reply != null) {
      const p = pending.get(m.reply);
      if (!p) return;
      pending.delete(m.reply);
      if (m.error != null) p.reject(new Error(m.error));
      else p.resolve(m.result);
    } else if (m.event) {
      for (const cb of listeners.get(m.event) ?? []) cb(m.params);
      for (const cb of listeners.get("*") ?? []) cb(m.event, m.params);
    }
  });

  function request(op, payload) {
    return new Promise((resolve, reject) => {
      const id = ++next;
      pending.set(id, { resolve, reject });
      window.parent.postMessage({ tuxhub: 1, id, op, ...payload }, "*");
    });
  }

  window.tuxhub = {
    call: (method, params = {}) => request("call", { method, params }),
    action: (action) => request("action", { action }),
    getSettings: () => request("getSettings", {}),
    saveSettings: (values) => request("saveSettings", { values }),
    launch: () => request("launch", {}),
    on(event, cb) {
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event).add(cb);
      return () => listeners.get(event).delete(cb);
    },
  };

  window.parent.postMessage({ tuxhub: 1, op: "ready" }, "*");
})();
