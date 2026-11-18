// Stored settings of an app's own menu (src/lib/apps/menu-store.ts): sending
// over `settings.set`, refusals, changes from outside. Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { StoredSettings } from "../src/lib/apps/menu-store.ts";

/** A fake app: answers `settings.*` and records the requests. */
function fakeApp(values = { a: 1, b: "x" }) {
  const app = { values: { ...values }, calls: [], refuse: null, adjust: null, hold: false, held: [], closed: false };
  app.call = (method, params) => {
    app.calls.push({ method, params });
    if (app.closed) return Promise.reject({ code: "closed", message: "gone" });
    const answer = () => {
      if (method === "settings.get") return { values: { ...app.values }, overlay: { mode: "x11", preference: "auto" }, version: "2.1.0" };
      if (method === "settings.reset") {
        app.values = { a: 0, b: "" };
        return true;
      }
      if (app.refuse) throw { code: "rejected", message: app.refuse };
      const value = app.adjust ? app.adjust(params.value) : params.value;
      app.values[params.key] = value;
      return { value };
    };
    if (app.hold && method === "settings.set") {
      return new Promise((resolve, reject) => app.held.push(() => {
        try {
          resolve(answer());
        } catch (e) {
          reject(e);
        }
      }));
    }
    return Promise.resolve().then(answer);
  };
  app.sets = () => app.calls.filter((c) => c.method === "settings.set");
  return app;
}

const tick = () => new Promise((r) => setTimeout(r, 0));

async function loaded(app, options = {}) {
  const refusals = [];
  const store = new StoredSettings(app.call, { onRefused: (key, message) => refusals.push({ key, message }), ...options });
  await store.load();
  return { store, refusals };
}

test("load takes the values, the overlay state and the version", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  assert.deepEqual(store.values, { a: 1, b: "x" });
  assert.equal(store.version, "2.1.0");
  assert.deepEqual(store.overlay, { mode: "x11", preference: "auto" });
});

test("edge case 3: a refused value goes back to the confirmed one, with the app's text", async () => {
  const app = fakeApp();
  const { store, refusals } = await loaded(app);
  app.refuse = "too <b>long</b>";
  store.set("b", "y", 0);
  assert.equal(store.values.b, "y");
  await tick();
  await tick();
  assert.equal(store.values.b, "x");
  assert.deepEqual(refusals, [{ key: "b", message: "too <b>long</b>" }]);
});

test("edge case 4: a value the app adjusted is shown as the app has it", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  app.adjust = (v) => Math.min(v, 10);
  store.set("a", 50, 0);
  await tick();
  await tick();
  assert.equal(store.values.a, 10);
});

test("edge case 5: one request per key at a time, the last value wins", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  app.hold = true;
  for (let i = 2; i <= 11; i++) store.set("a", i, 0);
  await tick();
  assert.equal(app.sets().length, 1);
  app.held.shift()();
  await tick();
  await tick();
  assert.equal(app.sets().length, 2);
  assert.equal(app.sets()[1].params.value, 11);
  app.held.shift()();
  await tick();
  await tick();
  assert.equal(store.values.a, 11);
  assert.equal(app.sets().length, 2);
});

test("edge case 6: a change from outside waits while the field has focus", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  store.focus("b");
  store.changed({ b: "outside" });
  assert.equal(store.values.b, "x");
  store.blur("b");
  assert.equal(store.values.b, "outside");
});

test("a change from outside does not overwrite a value that is still on its way", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  app.hold = true;
  store.set("a", 5, 0);
  store.changed({ a: 7 });
  assert.equal(store.values.a, 5);
  app.held.shift()();
  await tick();
  await tick();
  assert.equal(store.values.a, 5);
});

test("edge case 7: unknown keys from outside are ignored", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  store.changed({ nope: 1, a: 3 });
  assert.equal("nope" in store.values, false);
  assert.equal(store.values.a, 3);
});

test("edge case 8: a value the menu cannot show is not taken over", async () => {
  const app = fakeApp();
  const rejected = [];
  const { store } = await loaded(app, {
    accept: (key, value) => key !== "b" || typeof value === "string",
    onUnusable: (key) => rejected.push(key),
  });
  store.changed({ b: 42 });
  assert.equal(store.values.b, "x");
  assert.deepEqual(rejected, ["b"]);
});

test("the app's answer to a set is checked and rebuilt like every value from the app", async () => {
  const app = fakeApp();
  const rejected = [];
  const { store } = await loaded(app, {
    accept: (key, value) => key !== "b" || typeof value === "string",
    normalize: (key, value) => (key === "b" ? String(value).toUpperCase() : value),
    onUnusable: (key) => rejected.push(key),
  });
  // Loaded values are rebuilt too.
  assert.equal(store.values.b, "X");
  app.adjust = () => 42;
  store.set("b", "y", 0);
  await tick();
  await tick();
  // The answer 42 cannot be shown: the value the menu sent stays.
  assert.equal(store.values.b, "y");
  assert.deepEqual(rejected, ["b"]);
  app.adjust = (v) => `${v}!`;
  store.set("b", "z", 0);
  await tick();
  await tick();
  assert.equal(store.values.b, "Z!");
});

test("debounced input sends once after the pause, and at once on blur", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  store.focus("b");
  store.set("b", "y", 50);
  store.set("b", "yz", 50);
  assert.equal(app.sets().length, 0);
  store.blur("b");
  await tick();
  assert.equal(app.sets().length, 1);
  assert.equal(app.sets()[0].params.value, "yz");
});

test("reset asks the app to reset everything and loads again", async () => {
  const app = fakeApp();
  const { store } = await loaded(app);
  await store.resetAll();
  assert.deepEqual(app.calls.map((c) => c.method), ["settings.get", "settings.reset", "settings.get"]);
  assert.deepEqual(store.values, { a: 0, b: "" });
});

test("edge case 16: a lost connection is reported and nothing waits afterwards", async () => {
  const app = fakeApp();
  const closed = [];
  const { store } = await loaded(app, { onClosed: () => closed.push(true) });
  app.hold = true;
  store.set("a", 5, 0);
  store.set("a", 6, 0);
  const done = app.held.shift();
  app.hold = false;
  app.closed = true;
  store.cancel();
  done();
  await tick();
  await tick();
  // The cancelled value 6 is not sent after the answer of 5.
  assert.equal(app.sets().length, 1);
  assert.equal(closed.length, 0);
  store.set("a", 7, 0);
  await tick();
  await tick();
  assert.equal(closed.length, 1);
});
