// tuxly bridge for tool interfaces.
//
// Include it in the tool's HTML page:
//   <script src="/__tuxly/bridge.js"></script>
//
// It provides `window.tuxly`:
//   await tuxly.call("method", { ... })    // JSON-RPC call to the tool's rpc process
//   await tuxly.action("toggle")           // run an action declared in tuxly.toml
//   await tuxly.getSettings()              // saved settings
//   await tuxly.saveSettings({ key: 1 })   // save and send to the process
//   await tuxly.launch()                   // launch the tool in its own window
//   tuxly.on("status", (params) => …)      // notifications from the process
//   tuxly.on("theme", ({ dark }) => …)     // light or dark appearance of tuxly
//   tuxly.on("locale", ({ locale }) => …)  // user interface language, for example "en" or "de"
(() => {
  if (window.tuxly) return;
  const pending = new Map();
  const listeners = new Map();
  let next = 0;

  window.addEventListener("message", (ev) => {
    if (ev.source !== window.parent) return;
    const m = ev.data;
    if (!m || m.tuxly !== 1) return;
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
      window.parent.postMessage({ tuxly: 1, id, op, ...payload }, "*");
    });
  }

  window.tuxly = {
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

  window.parent.postMessage({ tuxly: 1, op: "ready" }, "*");
})();
