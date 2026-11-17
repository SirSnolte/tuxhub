// The workshop's model: every app once, areas, families, search, quick views
// and sorting (src/lib/workshop-model.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  AREAS,
  CHIPS,
  areaOf,
  chipCounts,
  chipFilter,
  entries,
  insertAt,
  matches,
  sortGroups,
} from "../src/lib/workshop-model.ts";

/** A start menu app as `local_apps_list` returns it. */
function view(desktop_id, source, extra = {}) {
  const { name, categories = [], system = false, ...rest } = extra;
  return {
    app: {
      desktop_id,
      name: name ?? desktop_id.replace(/\.desktop$/, ""),
      summary: null,
      description: null,
      icon: null,
      homepage: null,
      repo: null,
      categories,
      source,
      command: [],
      autostart: false,
      system,
    },
    tool_id: "local." + desktop_id.replace(/\.desktop$/, ""),
    in_sidebar: false,
    update: null,
    run_state: "stopped",
    integration: "embedded",
    ...rest,
  };
}
const rpm = (pkg) => ({ kind: "rpm", package: pkg });
const flatpak = (id) => ({ kind: "flatpak", flatpak_id: id, installations: ["user"], origin: "flathub" });
const unknown = (program) => ({ kind: "unknown", program });
const game = (id) => ({ kind: "game", store: "steam", game: id });

function item(id, source, extra = {}) {
  return {
    entry: { id, name: extra.name ?? id, description: "From the catalog", source },
    installed: false,
    pinned: false,
    present: false,
    integration: "socket",
    ...extra,
  };
}

const build = (local, catalog = [], more = {}) =>
  entries({ catalog, local, tools: [], updates: {}, overrides: {}, ...more });

const LABELS_EN = {
  area: (a) => ({ gaming: "Gaming tools & launchers", games: "Games", system: "System & utilities" })[a] ?? a,
  source: (s) => ({ flatpak: "Flatpak", rpm: "Package", game: "Steam" })[s] ?? s,
};
const LABELS_DE = {
  area: (a) => ({ gaming: "Gaming-Werkzeuge & Launcher", games: "Spiele", system: "System & Hilfsprogramme" })[a] ?? a,
  source: LABELS_EN.source,
};

// 1
test("a catalog app that is also installed is one entry among the tuxhub tools", () => {
  const sc = view("com.core447.StreamController.desktop", flatpak("com.core447.StreamController"), {
    tool_id: "streamcontroller",
  });
  const list = build(
    [sc, view("gimp.desktop", rpm("gimp"))],
    [
      item("streamcontroller", { type: "flatpak", flatpak_id: "com.core447.StreamController" }, { installed: true }),
      item("autoclicker", { type: "github-release", asset: "x" }),
    ],
  );
  const scs = list.filter((e) => e.members.some((m) => m.name.includes("StreamController") || m.toolId === "streamcontroller"));
  assert.equal(scs.length, 1);
  assert.equal(scs[0].catalog, true);
  assert.equal(scs[0].lead.desktopId, "com.core447.StreamController.desktop");
  // Not installed catalog apps stand there too.
  const ac = list.find((e) => e.lead.toolId === "autoclicker");
  assert.equal(ac.catalog, true);
  assert.equal(ac.installed, false);
  assert.equal(list.find((e) => e.lead.desktopId === "gimp.desktop").catalog, false);
});

// Matched by source too, when the app was added under its own id before.
test("a catalog app matches its start menu app by Flatpak id", () => {
  const sc = view("sc.desktop", flatpak("com.core447.StreamController"), { tool_id: "local.sc" });
  const list = build([sc], [item("streamcontroller", { type: "flatpak", flatpak_id: "com.core447.StreamController" })]);
  assert.equal(list.length, 1);
  assert.equal(list[0].catalog, true);
});

// 2
test("LibreOffice's five start menu entries are one family led by Writer", () => {
  const lo = ["base", "calc", "draw", "impress", "writer"].map((n) =>
    view(`libreoffice-${n}.desktop`, rpm("libreoffice-core"), { name: `LibreOffice ${n}`, categories: ["Office"] }),
  );
  const list = build(lo);
  assert.equal(list.length, 1);
  assert.equal(list[0].members.length, 5);
  assert.equal(list[0].lead.desktopId, "libreoffice-writer.desktop");
  assert.equal(list[0].name, "LibreOffice");
  assert.equal(list[0].area, "office");
});

// 3
test("a family with one installed member is a normal entry", () => {
  const list = build([view("libreoffice-calc.desktop", rpm("libreoffice-core"), { name: "LibreOffice Calc" })]);
  assert.equal(list.length, 1);
  assert.equal(list[0].members.length, 1);
  assert.equal(list[0].name, "LibreOffice Calc");
  assert.equal(list[0].key, "desktop:libreoffice-calc.desktop");
});

// 4
test("Wine's tools without a category are the family Wine among the system apps", () => {
  const wine = [
    view("wine-notepad.desktop", rpm("wine-core"), { name: "Wine Notepad", categories: ["X-Wine"], system: true }),
    view("wine-winecfg.desktop", rpm("wine-core"), { name: "Wine Configuration", categories: ["X-Wine"], system: true }),
    view("winetricks.desktop", rpm("winetricks"), { name: "Winetricks", categories: ["Utility"] }),
  ];
  const list = build(wine);
  assert.equal(list.length, 1);
  assert.equal(list[0].name, "Wine");
  assert.equal(list[0].members.length, 3);
  assert.equal(list[0].lead.desktopId, "wine-winecfg.desktop");
  assert.equal(list[0].area, "system");
});

test("other families: Kontact, KDE Connect, a Flatpak's entries, local starters of one folder", () => {
  const list = build([
    view("org.kde.kmail2.desktop", rpm("kmail"), { name: "KMail", categories: ["Network"] }),
    view("org.kde.korganizer.desktop", rpm("korganizer"), { name: "KOrganizer", categories: ["Office"] }),
    view("org.kde.kdeconnect.app.desktop", rpm("kdeconnectd"), { name: "KDE Connect" }),
    view("org.kde.kdeconnect.sms.desktop", rpm("kdeconnectd"), { name: "KDE Connect SMS" }),
    view("chat.desktop", flatpak("org.example.Chat"), { name: "Chat" }),
    view("chat-settings.desktop", flatpak("org.example.Chat"), { name: "Chat Settings" }),
    view("couchmode-on.desktop", unknown("/home/u/bin/couchmode-on"), { name: "Couch mode on" }),
    view("couchmode-off.desktop", unknown("/home/u/bin/couchmode-off"), { name: "Couch mode off" }),
    view("couchmode-x.desktop", unknown("/opt/other/couchmode-x"), { name: "Elsewhere" }),
  ]);
  const by = (name) => list.find((e) => e.name === name);
  assert.equal(by("Kontact").members.length, 2);
  assert.equal(by("Kontact").lead.desktopId, "org.kde.kmail2.desktop");
  assert.equal(by("KDE Connect").members.length, 2);
  assert.equal(by("Chat").members.length, 2);
  assert.equal(by("Couch mode").members.length, 2);
  assert.ok(by("Elsewhere"));
  assert.equal(list.length, 5);
});

// 5
test("a Steam game is in Games, not System, with the source game", () => {
  const g = view("Portal 2.desktop", game("620"), { categories: ["Game"], system: false });
  const list = build([g]);
  assert.equal(list[0].area, "games");
  assert.equal(list[0].lead.source, "game");
  assert.equal(list[0].level, 1);
  // Also without a category.
  assert.equal(areaOf({ desktopId: "x.desktop", source: "game", sourceId: "1", categories: [], system: true }, {}), "games");
});

// 6
test("the Steam client and other gaming tools are in Gaming", () => {
  const a = (desktopId, source, sourceId, categories = ["Game"]) =>
    areaOf({ desktopId, source, sourceId, categories, system: false }, {});
  assert.equal(a("steam.desktop", "unknown", "/usr/bin/steam"), "gaming");
  assert.equal(a("com.heroicgameslauncher.hgl.desktop", "flatpak", "com.heroicgameslauncher.hgl"), "gaming");
  assert.equal(a("net.davidotek.pupgui2.desktop", "flatpak", "net.davidotek.pupgui2", ["Utility"]), "gaming");
  assert.equal(a("org.openrgb.OpenRGB.desktop", "rpm", "openrgb", ["Utility"]), "gaming");
  // Known file managers, terminals and the system monitor are tools.
  assert.equal(a("org.kde.dolphin.desktop", "rpm", "dolphin", ["System", "FileManager"]), "tools");
  assert.equal(a("org.kde.konsole.desktop", "rpm", "konsole", ["System", "TerminalEmulator"]), "tools");
  // Categories, then the system flag, then tools.
  assert.equal(a("code.desktop", "rpm", "code", ["Development"]), "dev");
  assert.equal(a("vlc.desktop", "rpm", "vlc", ["AudioVideo"]), "media");
  assert.equal(a("x.desktop", "rpm", "x", ["Network"]), "comm");
  assert.equal(a("x.desktop", "rpm", "x", ["Graphics"]), "graphics");
  assert.equal(areaOf({ desktopId: "s.desktop", source: "rpm", sourceId: "s", categories: ["Settings"], system: true }, {}), "system");
  assert.equal(a("x.desktop", "rpm", "x", []), "tools");
});

// 7
test("the user's area wins over the rules; none resets it", () => {
  const input = { desktopId: "gimp.desktop", source: "rpm", sourceId: "gimp", categories: ["Graphics"], system: false };
  assert.equal(areaOf(input, { "gimp.desktop": "media" }), "media");
  assert.equal(areaOf(input, {}), "graphics");
  // An unknown area is ignored.
  assert.equal(areaOf(input, { "gimp.desktop": "nope" }), "graphics");
  const list = build([view("gimp.desktop", rpm("gimp"), { categories: ["Graphics"] })], [], {
    overrides: { "gimp.desktop": "media" },
  });
  assert.equal(list[0].area, "media");
});

// 8
test("the search finds a family by a member, ignoring case and outer spaces", () => {
  const lo = ["calc", "writer"].map((n) =>
    view(`libreoffice-${n}.desktop`, rpm("libreoffice-core"), { name: `LibreOffice ${n === "calc" ? "Calc" : "Writer"}` }),
  );
  const list = build([...lo, view("gimp.desktop", rpm("gimp"))]);
  const found = list.filter((e) => matches(e, "  CALC ", LABELS_EN));
  assert.equal(found.length, 1);
  assert.equal(found[0].name, "LibreOffice");
  assert.equal(list.filter((e) => matches(e, "", LABELS_EN)).length, 2);
  // Desktop id and package count too.
  assert.equal(list.filter((e) => matches(e, "libreoffice-core", LABELS_EN)).length, 1);
});

// 9
test("the search finds by source and by area name in the shown language", () => {
  const list = build([
    view("chat.desktop", flatpak("org.example.Chat"), { categories: ["Network"] }),
    view("Portal 2.desktop", game("620"), { categories: ["Game"] }),
  ]);
  assert.deepEqual(list.filter((e) => matches(e, "flatpak", LABELS_EN)).map((e) => e.name), ["chat"]);
  assert.deepEqual(list.filter((e) => matches(e, "games", LABELS_EN)).map((e) => e.name), ["Portal 2"]);
  assert.deepEqual(list.filter((e) => matches(e, "spiele", LABELS_DE)).map((e) => e.name), ["Portal 2"]);
  assert.equal(list.filter((e) => matches(e, "spiele", LABELS_EN)).length, 0);
});

// 10
test("quick view counts match the filtered lists", () => {
  const list = build(
    [
      view("a.desktop", rpm("a"), { in_sidebar: true, run_state: "own", integration: "embedded" }),
      view("b.desktop", rpm("b"), { update: "2.0", integration: "standalone" }),
      view("c.desktop", game("1"), { run_state: "own" }),
      view("e.desktop", rpm("e"), { run_state: "unknown", integration: null }),
      view("d.desktop", flatpak("org.d.D"), { update: "3", in_sidebar: true }),
    ],
    [item("auto", { type: "github-release", asset: "x" }, { installed: true, pinned: true })],
    { tools: [], updates: { auto: "1.1" } },
  );
  const counts = chipCounts(list);
  assert.deepEqual(CHIPS, ["all", "pinned", "running", "updates", "l3", "l2", "l1"]);
  for (const chip of CHIPS) {
    assert.equal(counts[chip], list.filter(chipFilter(chip)).length, chip);
  }
  // "unknown" is not counted as running.
  assert.deepEqual(counts, { all: 6, pinned: 3, running: 2, updates: 3, l3: 1, l2: 3, l1: 2 });
});

// 11
test("tuxhub's own entry never shows", () => {
  const list = build(
    [view("tuxhub.desktop", rpm("tuxhub"), { tool_id: "tuxhub" }), view("gimp.desktop", rpm("gimp"))],
    [item("tuxhub", { type: "github-release", asset: "x" })],
  );
  assert.deepEqual(list.map((e) => e.lead.desktopId), ["gimp.desktop"]);
});

// 12
test("insertAt clamps the place and never adds an id twice", () => {
  assert.deepEqual(insertAt(["a", "b"], ["x"], 99), ["a", "b", "x"]);
  assert.deepEqual(insertAt(["a", "b"], ["x"], -3), ["x", "a", "b"]);
  assert.deepEqual(insertAt(["a", "b"], ["x", "y"], 1), ["a", "x", "y", "b"]);
  assert.deepEqual(insertAt(["a", "b"], ["b", "x"], 0), ["x", "a", "b"]);
  assert.deepEqual(insertAt(["a"], ["x", "x"], null), ["a", "x"]);
  const order = ["a"];
  insertAt(order, ["x"], 0);
  assert.deepEqual(order, ["a"], "the input stays as it is");
});

test("groups follow the sort: areas in their order, A-Z, by integration, by source", () => {
  const list = build([
    view("z.desktop", rpm("z"), { name: "Zed", categories: ["Development"] }),
    view("a.desktop", rpm("a"), { name: "alpha", categories: ["Development"] }),
    view("g.desktop", game("5"), { name: "Game" }),
    view("s.desktop", rpm("s"), { name: "Settings", categories: ["Settings"], system: true }),
  ], [item("auto", { type: "github-release", asset: "x" })]);
  const byArea = sortGroups(list, "area");
  assert.deepEqual(byArea.map((g) => g.key), ["games", "dev", "system"]);
  assert.deepEqual(byArea[1].entries.map((e) => e.name), ["alpha", "Zed"]);
  assert.ok(byArea.every((g) => g.entries.every((e) => !e.catalog)), "catalog apps have their own section");
  assert.deepEqual(AREAS.indexOf("games") < AREAS.indexOf("dev"), true);
  const az = sortGroups(list, "name");
  assert.deepEqual(az.map((g) => g.key), ["all"]);
  assert.deepEqual(az[0].entries.map((e) => e.name), ["alpha", "Game", "Settings", "Zed"]);
  assert.deepEqual(sortGroups(list, "level").map((g) => g.key), ["l2", "l1"]);
  assert.deepEqual(sortGroups(list, "source").map((g) => g.key), ["s:rpm", "s:game"]);
});
