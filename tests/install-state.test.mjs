// What the workshop and a tool page offer for apps that are already on the
// computer (src/lib/install-state.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { asksHowToRemove, installHint, needsPassword, workshopAction } from "../src/lib/install-state.ts";

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
