// What the workshop and a tool page offer for apps that are already on the
// computer (src/lib/install-state.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  asksHowToRemove,
  installHint,
  markInSidebar,
  needsPassword,
  tuxhubInstalls,
  workshopAction,
} from "../src/lib/install-state.ts";

test("the workshop opens tools in tuxhub, adds apps on the computer and installs the rest", () => {
  assert.equal(workshopAction({ installed: true, present: false }), "open");
  assert.equal(workshopAction({ installed: false, present: true }), "adopt");
  assert.equal(workshopAction({ installed: false, present: false }), "install");
});

test("a missing app is pointed out before a second installation", () => {
  assert.equal(installHint({ missing: false, duplicate: false }), null);
  assert.equal(installHint({ missing: false, duplicate: true }), "duplicate");
  assert.equal(installHint({ missing: true, duplicate: false }), "missing");
  assert.equal(installHint({ missing: true, duplicate: true }), "missing");
});

test("only an adopted app that is still there asks how to remove it", () => {
  assert.equal(asksHowToRemove({ adopted: true, missing: false }), true);
  assert.equal(asksHowToRemove({ adopted: false, missing: false }), false);
  assert.equal(asksHowToRemove({ adopted: true, missing: true }), false);
});

test("changes to system-wide installations ask for the password", () => {
  const flatpak = { type: "flatpak", flatpak_id: "net.example.App" };
  assert.equal(needsPassword({ entry: { source: flatpak }, installation: "system" }), true);
  assert.equal(needsPassword({ entry: { source: flatpak }, installation: "user" }), false);
  const pkg = { type: "system-package", package: "clock", command: "/usr/bin/clock" };
  assert.equal(needsPassword({ entry: { source: pkg }, installation: "user" }), true);
  assert.equal(needsPassword({ entry: { source: { type: "github-release", asset: "*" } }, installation: "user" }), false);
});

test("tuxhub neither installs nor uninstalls distribution packages and found apps", () => {
  const sources = [
    [{ type: "distro-package", package: "obs-studio", manager: "rpm", command: ["obs"] }, false],
    [{ type: "launcher", desktop_id: "a.desktop", command: ["/opt/a/a"] }, false],
    [{ type: "flatpak", flatpak_id: "net.example.App" }, true],
    [{ type: "system-package", package: "clock", command: "/usr/bin/clock" }, true],
    [{ type: "github-release", asset: "a.tar.gz" }, true],
  ];
  for (const [source, expected] of sources) {
    assert.equal(tuxhubInstalls({ entry: { source } }), expected, source.type);
  }
});

test("the catalog follows a switch in the installed tab without loading again", () => {
  const item = (id, installed, present) => ({ entry: { id, name: id }, installed, present });
  const items = [item("demo", false, true), item("other", false, false)];
  const added = markInSidebar(items, "demo", true);
  assert.deepEqual(added.map((i) => [i.entry.id, i.installed, i.present]), [
    ["demo", true, true],
    ["other", false, false],
  ]);
  assert.equal(items[0].installed, false, "the old list is left alone");
  // Removed from the sidebar, the app is still on the computer.
  const removed = markInSidebar([item("demo", true, false)], "demo", false);
  assert.deepEqual(removed.map((i) => [i.installed, i.present]), [[false, true]]);
  // An app the catalog does not know changes nothing.
  assert.deepEqual(markInSidebar(items, "local.x", true), items);
});
