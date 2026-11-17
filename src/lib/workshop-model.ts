// The workshop's model, without Svelte: every app of the catalog and the
// start menu once, in areas, start menu entries of one program as a family,
// and the search, quick views and sorting the page shows them with.
import type { Integration, LocalAppView, Tool, WorkshopItem } from "./api";

export const AREAS = ["gaming", "games", "comm", "media", "office", "graphics", "tools", "dev", "system"] as const;
export type Area = (typeof AREAS)[number];
export type Level = 1 | 2 | 3;

/** Where an app comes from: a start menu source, or a tool tuxhub installed. */
export type SourceKind = LocalAppView["app"]["source"]["kind"] | "tuxhub";
export const SOURCES: SourceKind[] = ["tuxhub", "flatpak", "rpm", "deb", "appimage", "unknown", "game"];

/** One app: a start menu entry, a catalog app or a tool in tuxhub. */
export type Member = {
  /** `desktop:<desktop id>` or `tool:<tool id>`. */
  key: string;
  desktopId: string | null;
  toolId: string;
  name: string;
  summary: string;
  icon: string | null;
  source: SourceKind;
  /** Package, Flatpak id or program. */
  sourceId: string;
  categories: string[];
  system: boolean;
  area: Area;
  level: Level;
  installed: boolean;
  pinned: boolean;
  running: boolean;
  update: string | null;
  view?: LocalAppView;
  item?: WorkshopItem;
  tool?: Tool;
};

/** A tile of the workshop: an app or a family of apps. */
export type WEntry = {
  key: string;
  lead: Member;
  members: Member[];
  name: string;
  /** A catalog app or a tool of tuxhub: shown among the tuxhub tools. */
  catalog: boolean;
  area: Area;
  level: Level;
  installed: boolean;
  pinned: boolean;
  running: boolean;
  update: string | null;
};

export type AreaInput = Pick<Member, "desktopId" | "source" | "sourceId" | "categories" | "system">;

const stem = (desktopId: string | null) => (desktopId ?? "").replace(/\.desktop$/, "");
const lower = (s: string) => s.toLowerCase();

/** Whether `id` is `key` or starts with it and a separator. */
const startsAs = (id: string, key: string) =>
  id === key || id.startsWith(key + ".") || id.startsWith(key + "-") || id.startsWith(key + "_");

/** Launchers and tools for gaming, by desktop file name, Flatpak id or package. */
const GAMING = [
  "steam", "com.valvesoftware.steam", "heroic", "com.heroicgameslauncher", "lutris", "net.lutris",
  "prismlauncher", "org.prismlauncher", "protonup-qt", "net.davidotek.pupgui2", "mangojuice",
  "io.github.radiolamp.mangojuice", "mangohud", "goverlay", "io.github.benjamimgois.goverlay", "openrgb",
  "org.openrgb", "piper", "org.freedesktop.piper", "easyeffects", "com.github.wwmm.easyeffects", "dosbox",
  "io.github.dosbox", "io.github.dosbox-staging",
];
/** File managers, terminals and the system monitor: tools, not system apps. */
const TOOLS = [
  "org.kde.dolphin", "dolphin", "org.kde.konsole", "konsole", "org.kde.plasma-systemmonitor",
  "plasma-systemmonitor", "org.gnome.nautilus", "nautilus", "org.gnome.ptyxis", "org.gnome.console",
  "org.gnome.systemmonitor",
];

/** The area of an app: the user's choice, else fixed rules in this order. */
export function areaOf(app: AreaInput, overrides: Record<string, string>): Area {
  const chosen = app.desktopId ? overrides[app.desktopId] : undefined;
  if (chosen && (AREAS as readonly string[]).includes(chosen)) return chosen as Area;
  if (app.source === "game") return "games";
  const ids = [stem(app.desktopId), app.sourceId.split("/").pop() ?? ""].map(lower).filter(Boolean);
  if (ids.some((id) => GAMING.some((k) => startsAs(id, k)))) return "gaming";
  if (ids.some((id) => TOOLS.includes(id))) return "tools";
  const has = (c: string) => app.categories.includes(c);
  if (has("Game")) return "games";
  if (has("Development")) return "dev";
  if (has("AudioVideo") || has("Audio") || has("Video")) return "media";
  if (has("Network")) return "comm";
  if (has("Office")) return "office";
  if (has("Graphics")) return "graphics";
  if (has("Utility")) return "tools";
  if (app.system) return "system";
  return "tools";
}

/** Families with a fixed name and lead: matched by package or desktop file name. */
const FIXED_FAMILIES: { name: string; lead: string; test: (m: Member) => boolean }[] = [
  {
    name: "LibreOffice",
    lead: "libreoffice-writer",
    test: (m) => startsAs(lower(m.sourceId), "libreoffice") || startsAs(lower(stem(m.desktopId)), "libreoffice"),
  },
  {
    name: "Wine",
    lead: "wine-winecfg",
    test: (m) => {
      const ids = [lower(m.sourceId), lower(stem(m.desktopId))];
      return ids.some((id) => startsAs(id, "wine") || id === "winetricks");
    },
  },
  { name: "VMware", lead: "vmware-workstation", test: (m) => /^vmware[-_]/.test(lower(stem(m.desktopId))) },
  { name: "Qt 6", lead: "qt6-designer", test: (m) => lower(stem(m.desktopId)).startsWith("qt6-") },
  {
    name: "KDE Connect",
    lead: "org.kde.kdeconnect.app",
    test: (m) => lower(stem(m.desktopId)).includes("kdeconnect"),
  },
  {
    name: "Kontact",
    lead: "org.kde.kmail2",
    test: (m) =>
      /^org\.kde\.(kmail2|kontact|korganizer|kaddressbook|headerthemeeditor|contactthemeeditor|contactprintthemeeditor|sieveeditor|pimdataexporter|akonadiimportwizard|ktnef)$/.test(
        stem(m.desktopId),
      ),
  },
];

/** Desktop file names browsers give their web apps: `<browser>-<app id>-<profile>`. */
const WEB_APP = /-(Default|Profile_\d+)$/;

/** The folder of a program path, or "" for a bare name. */
const folderOf = (program: string) => (program.includes("/") ? program.slice(0, program.lastIndexOf("/")) : "");

/** The key that puts an app into a family, or null. */
function familyKey(m: Member): string | null {
  const fixed = FIXED_FAMILIES.find((f) => f.test(m));
  if (fixed) return "fixed:" + fixed.name;
  if (m.source === "flatpak") return "flatpak:" + m.sourceId;
  // Local starters only: a browser's web apps (`brave-<id>-Default`) share a
  // prefix and a program, but are apps of their own.
  if (m.source === "unknown" && !WEB_APP.test(stem(m.desktopId))) {
    const s = stem(m.desktopId);
    const dash = s.indexOf("-");
    const folder = folderOf(m.sourceId);
    if (dash > 0 && folder) return `local:${s.slice(0, dash)}:${folder}`;
  }
  return null;
}

/** The start of `names` they all share, without a trailing separator. */
function commonName(names: string[]): string {
  let prefix = names[0] ?? "";
  for (const n of names) {
    while (prefix && !n.startsWith(prefix)) prefix = prefix.slice(0, -1);
  }
  // Only whole words.
  const cut = prefix.match(/^(.*\S)[\s\-–:(]+\S*$/);
  if (!names.every((n) => n === prefix || /[\s\-–:(]/.test(n.charAt(prefix.length)))) {
    prefix = cut ? cut[1] : "";
  }
  return prefix.replace(/[\s\-–:(]+$/, "").trim();
}

/** Start menu entries of one program, grouped; each group sorted, lead first. */
export function families(members: Member[]): { name: string; members: Member[] }[] {
  const groups = new Map<string, Member[]>();
  const out: { key: string | null; members: Member[] }[] = [];
  for (const m of members) {
    const key = familyKey(m);
    if (key === null) {
      out.push({ key: null, members: [m] });
      continue;
    }
    const list = groups.get(key);
    if (list) list.push(m);
    else {
      const fresh = [m];
      groups.set(key, fresh);
      out.push({ key, members: fresh });
    }
  }
  return out.flatMap(({ key, members: list }) => {
    if (list.length < 2) return list.map((m) => ({ name: m.name, members: [m] }));
    const fixed = key?.startsWith("fixed:") ? FIXED_FAMILIES.find((f) => "fixed:" + f.name === key) : undefined;
    const sorted = [...list].sort((a, b) => a.name.localeCompare(b.name) || a.key.localeCompare(b.key));
    const lead = (fixed && sorted.find((m) => stem(m.desktopId) === fixed.lead)) || sorted[0];
    const name = fixed?.name || commonName(sorted.map((m) => m.name)) || lead.name;
    return [{ name, members: [lead, ...sorted.filter((m) => m !== lead)] }];
  });
}

const levelOf = (integration: Integration | null | undefined): Level =>
  integration === "socket" ? 3 : integration === "standalone" ? 1 : 2;

/** The id a source names: Flatpak id, package or program. */
function sourceIdOf(view: LocalAppView): string {
  const s = view.app.source;
  switch (s.kind) {
    case "flatpak":
      return s.flatpak_id;
    case "rpm":
    case "deb":
      return s.package;
    case "appimage":
      return s.path;
    case "unknown":
      return s.program;
    case "game":
      return s.game;
  }
}

/** Whether a catalog entry stands for the start menu app. */
function sameApp(item: WorkshopItem, view: LocalAppView): boolean {
  if (view.tool_id === item.entry.id) return true;
  const a = item.entry.source;
  const b = view.app.source;
  if (a.type === "flatpak") return b.kind === "flatpak" && b.flatpak_id === a.flatpak_id;
  if (a.type === "system-package" || a.type === "distro-package")
    return (b.kind === "rpm" || b.kind === "deb") && b.package === a.package;
  return false;
}

const OWN = "tuxhub";

export type EntriesInput = {
  catalog: WorkshopItem[];
  local: LocalAppView[];
  /** The tools of the sidebar. */
  tools: Tool[];
  /** Tool id -> new version. */
  updates: Record<string, string>;
  /** Desktop id -> area the user chose. */
  overrides: Record<string, string>;
};

function localMember(view: LocalAppView, overrides: Record<string, string>): Member {
  const source = view.app.source.kind;
  const base = {
    desktopId: view.app.desktop_id,
    source,
    sourceId: sourceIdOf(view),
    categories: view.app.categories,
    system: view.app.system,
  };
  return {
    ...base,
    key: "desktop:" + view.app.desktop_id,
    toolId: view.tool_id,
    name: view.app.name,
    summary: view.app.summary ?? "",
    icon: view.app.icon,
    area: areaOf(base, overrides),
    // A game runs through its store: tuxhub only starts it.
    level: source === "game" ? 1 : levelOf(view.integration),
    installed: true,
    pinned: view.in_sidebar,
    running: view.run_state === "hub" || view.run_state === "own",
    update: view.update,
    view,
  };
}

function entryOf(key: string, name: string, members: Member[], catalog: boolean): WEntry {
  const lead = members[0];
  return {
    key,
    lead,
    members,
    name,
    catalog,
    area: lead.area,
    level: lead.level,
    installed: members.some((m) => m.installed),
    pinned: members.some((m) => m.pinned),
    running: members.some((m) => m.running),
    update: members.find((m) => m.update)?.update ?? null,
  };
}

/** Every app once: catalog apps (with their start menu app) and tools of
 * tuxhub first, then the other start menu apps, families as one entry. */
export function entries(input: EntriesInput): WEntry[] {
  const { overrides, updates } = input;
  const local = input.local.filter((v) => stem(v.app.desktop_id) !== OWN && v.tool_id !== OWN);
  const used = new Set<LocalAppView>();
  const toolOf = (id: string) => input.tools.find((t) => t.entry.id === id);
  const out: WEntry[] = [];
  for (const item of input.catalog) {
    if (item.entry.id === OWN) continue;
    const view = local.find((v) => !used.has(v) && sameApp(item, v));
    if (view) used.add(view);
    const tool = toolOf(item.entry.id);
    const base = view ? localMember(view, overrides) : undefined;
    const member: Member = {
      key: "tool:" + item.entry.id,
      desktopId: view?.app.desktop_id ?? null,
      toolId: item.entry.id,
      name: item.entry.name,
      summary: item.entry.description,
      icon: item.entry.icon ?? base?.icon ?? null,
      source: base?.source ?? "tuxhub",
      sourceId: base?.sourceId ?? item.entry.id,
      categories: base?.categories ?? [],
      system: false,
      area: "gaming",
      level: levelOf(item.integration),
      installed: item.installed || !!view,
      pinned: item.pinned || !!view?.in_sidebar || !!tool,
      running: !!tool?.running || !!tool?.app_running || !!base?.running,
      update: updates[item.entry.id] ?? base?.update ?? null,
      view,
      item,
      tool,
    };
    out.push(entryOf(member.key, member.name, [member], true));
  }
  // Tools of the sidebar that are neither in the catalog nor start menu apps.
  for (const tool of input.tools) {
    const id = tool.entry.id;
    if (id === OWN || out.some((e) => e.lead.toolId === id) || local.some((v) => v.tool_id === id)) continue;
    const member: Member = {
      key: "tool:" + id,
      desktopId: null,
      toolId: id,
      name: tool.entry.name,
      summary: tool.entry.description,
      icon: tool.entry.icon ?? tool.manifest.icon ?? null,
      source: "tuxhub",
      sourceId: id,
      categories: [],
      system: false,
      area: "gaming",
      level: levelOf(tool.integration),
      installed: true,
      pinned: true,
      running: tool.running || tool.app_running,
      update: updates[id] ?? null,
      tool,
    };
    out.push(entryOf(member.key, member.name, [member], true));
  }
  const rest = local.filter((v) => !used.has(v)).map((v) => localMember(v, overrides));
  for (const f of families(rest)) {
    const key = f.members.length > 1 ? "family:" + f.members[0].key : f.members[0].key;
    out.push(entryOf(key, f.name, f.members, false));
  }
  return out;
}

export type Labels = { area: (a: Area) => string; source: (s: SourceKind) => string };

/** Whether the entry fits the search: name, summary, desktop id, package or
 * Flatpak id, source and area name of any member; case and outer spaces do
 * not matter. */
export function matches(e: WEntry, query: string, labels: Labels): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const text = [e.name, labels.area(e.area)];
  for (const m of e.members) {
    text.push(m.name, m.summary, m.desktopId ?? "", m.sourceId, m.toolId, labels.source(m.source), labels.area(m.area));
  }
  return text.join("\n").toLowerCase().includes(q);
}

export const CHIPS = ["all", "pinned", "running", "updates", "l3", "l2", "l1"] as const;
export type Chip = (typeof CHIPS)[number];

/** The test of a quick view. */
export function chipFilter(chip: Chip): (e: WEntry) => boolean {
  switch (chip) {
    case "all":
      return () => true;
    case "pinned":
      return (e) => e.pinned;
    case "running":
      return (e) => e.running;
    case "updates":
      return (e) => e.update !== null;
    default: {
      const level = Number(chip.slice(1));
      return (e) => e.level === level;
    }
  }
}

export function chipCounts(list: WEntry[]): Record<Chip, number> {
  return Object.fromEntries(CHIPS.map((c) => [c, list.filter(chipFilter(c)).length])) as Record<Chip, number>;
}

export type Sort = "area" | "name" | "level" | "source";
export type Group = { key: string; entries: WEntry[] };

const byName = (a: WEntry, b: WEntry) =>
  a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || a.key.localeCompare(b.key);

/** The entries outside the tuxhub tools, grouped by the sort: areas in their
 * order, one A–Z list, levels 3 to 1, or sources. Empty groups are left out;
 * each group is sorted by name. */
export function sortGroups(list: WEntry[], sort: Sort): Group[] {
  const rest = list.filter((e) => !e.catalog).sort(byName);
  let groups: Group[];
  switch (sort) {
    case "name":
      groups = [{ key: "all", entries: rest }];
      break;
    case "level":
      groups = [3, 2, 1].map((l) => ({ key: "l" + l, entries: rest.filter((e) => e.level === l) }));
      break;
    case "source":
      groups = SOURCES.map((s) => ({ key: "s:" + s, entries: rest.filter((e) => e.lead.source === s) }));
      break;
    default:
      groups = AREAS.map((a) => ({ key: a, entries: rest.filter((e) => e.area === a) }));
  }
  return groups.filter((g) => g.entries.length > 0);
}

/** `order` with the `ids` it does not have yet inserted at `at` (clamped to
 * the list; the end without), in their order. The input is not changed. */
export function insertAt(order: string[], ids: string[], at: number | null): string[] {
  const out = [...order];
  let i = Math.max(0, Math.min(at ?? out.length, out.length));
  for (const id of ids) {
    if (!out.includes(id)) out.splice(i++, 0, id);
  }
  return out;
}

export type ReasonKey =
  | "rm.why.protected"
  | "rm.why.tooMany"
  | "rm.why.dnfRefuses"
  | "rm.why.dnfSilent"
  | "rm.why.packageManager"
  | "rm.why.outsideHome"
  | "rm.why.notAPackage"
  | "rm.why.invalid"
  | "rm.why.other";

/** The text for a reason the backend gives for refusing an uninstall
 * (`RemovePreview.blocked`): its i18n key and the values it names. */
export function blockedReason(code: string): { key: ReasonKey; vars: Record<string, string> } {
  const [kind, ...rest] = code.split(":");
  const detail = rest.join(":");
  switch (kind) {
    case "protected":
      return { key: "rm.why.protected", vars: { package: detail } };
    case "too-many":
      return { key: "rm.why.tooMany", vars: { count: detail } };
    case "dnf-refuses":
      return { key: "rm.why.dnfRefuses", vars: { detail } };
    case "dnf-silent":
      return { key: "rm.why.dnfSilent", vars: {} };
    case "package-manager":
      return { key: "rm.why.packageManager", vars: {} };
    case "outside-home":
      return { key: "rm.why.outsideHome", vars: {} };
    case "not-a-package":
      return { key: "rm.why.notAPackage", vars: {} };
    case "invalid":
      return { key: "rm.why.invalid", vars: {} };
    default:
      return { key: "rm.why.other", vars: { detail: code } };
  }
}
