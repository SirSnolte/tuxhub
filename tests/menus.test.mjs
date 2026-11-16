// Which tools get tuxhub's own menu and when (src/lib/apps/menus.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { menuOf, menuStep, probeOutcome } from "../src/lib/apps/menus.ts";

test("only tools with an own menu get one", () => {
  assert.ok(menuOf("fliq"));
  assert.equal(menuOf("autoclicker"), undefined);
  assert.equal(menuOf("__proto__"), undefined);
});

test("edge case 1: an app that does not run is not asked for its settings", () => {
  for (const step of ["unreachable", "failure", "protocol"]) {
    assert.equal(menuStep(step, true), "status", step);
  }
  assert.equal(menuStep("load", true), "probe");
  assert.equal(menuStep("load", false), "form");
});

test("edge case 2: an app without settings.* keeps the generic page", () => {
  assert.equal(probeOutcome({ code: "rejected", message: 'unknown method "settings.get"' }), "form");
  assert.equal(probeOutcome({ code: "closed", message: "gone" }), "closed");
  assert.equal(probeOutcome({ code: "timeout", message: "slow" }), "failure");
  assert.equal(probeOutcome(new Error("x")), "failure");
});
