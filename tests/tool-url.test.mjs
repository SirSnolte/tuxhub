// Addresses of tool files per operating system (src/lib/tool-url.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { toolUrl } from "../src/lib/tool-url.ts";

test("Linux names the tool in the host", () => {
  assert.equal(toolUrl("linux", "fliq", "ui/index.html"), "tool://fliq/ui/index.html");
});

test("Windows names the tool in the path of tool.localhost", () => {
  assert.equal(toolUrl("windows", "fliq", "/ui/index.html"), "http://tool.localhost/fliq/ui/index.html");
});
