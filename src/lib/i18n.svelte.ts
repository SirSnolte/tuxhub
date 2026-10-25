// User interface strings. English is the reference; every key must exist in `en`.
// Placeholders use `{name}` and are filled by `t(key, { name })`.

const en = {
  "nav.workshop": "Workshop",
  "nav.myTools": "My tools",
  "nav.settings": "Settings",
  "nav.empty": "Nothing installed yet. Browse the workshop to get started.",
  "nav.updateOne": "Install 1 update",
  "nav.updateMany": "Install {count} updates",
  "nav.updating": "Updating…",
  "nav.updateTo": "Update available: {version}",
  "nav.active": "Active",

  "common.retry": "Try again",
  "common.cancel": "Cancel",
  "common.dismiss": "Dismiss",

  "category.input": "Input",
  "category.compatibility": "Compatibility",
  "category.overlay": "Overlay",
  "category.launcher": "Launcher",
  "category.creative": "Creative",
  "category.local": "Local",
  "category.other": "Other",

  "source.github-release": "GitHub release",
  "source.flatpak": "Flatpak",
  "source.local": "Local",
  "source.external": "Installed separately",
  "source.system-package": "System package",

  "level.0": "Standalone",
  "level.1": "Web interface",
  "level.2": "Integrated",
  "level.3": "Live",

  "tool.running": "Running",
  "tool.openWindow": "Open own window",
  "tool.started": "{name} started",
  "tool.checkUpdates": "Check for updates",
  "tool.isUpToDate": "{name} is up to date",
  "tool.updateTo": "Update to {version}",
  "tool.updatedTo": "Updated to {version}",
  "tool.stopProcess": "Stop process",
  "tool.reconnect": "Reconnect",
  "tool.ownWindow": "Open in window",
  "tool.uninstall": "Uninstall",
  "tool.unlink": "Unlink",
  "tool.confirmUninstall": "Uninstall {name}?",
  "tool.confirmUnlink": "Remove the link to {name}?",
  "tool.uninstallHint": "The tool, its settings and its hotkeys are removed from tuxhub.",
  "tool.unlinkHint": "Its settings are removed from tuxhub. The folder itself is not deleted.",
  "tool.removed": "{name} removed",
  "tool.noLauncher": "No executable was found for this tool.",
  "tool.unknownOp": "Unknown operation: {op}",
  "tool.more": "More actions",

  "embed.start": "Start in tuxhub",
  "embed.hint": "The application runs inside tuxhub. For 3D games, use Open in window.",
  "embed.starting": "Starting {name}…",
  "embed.stopping": "Closing {name}…",
  "embed.lost": "Connection to {name} lost",
  "embed.startedWindow": "{name} opened in its own window",
  "embed.missing": "Running applications inside tuxhub requires {dep}. Install it with your package manager:",

  "app.start": "Start",
  "app.quit": "Quit",
  "app.running": "{name} is running.",
  "app.stopped": "{name} is not running.",
  "app.quitDone": "{name} quit",
  "app.startToChange": "Start {name} to change its settings.",
  "app.hint": "{name} runs in its own window on the desktop, also when tuxhub is closed.",

  "web.checking": "Checking web interface…",
  "web.unreachable": "The web interface is not responding. Is {name} running?",
  "web.start": "Start {name}",
  "web.autoAppear": "It will appear here as soon as it is reachable.",
  "web.startedWaiting": "{name} started. Waiting for the web interface…",
  "web.wsHint": "Some live features that rely on WebSockets may only work in a separate window.",

  "readme.loading": "Loading README…",
  "readme.unavailable": "README unavailable: {error}",

  "settings.saved": "Saved",
  "settings.saving": "Saving…",

  "live.loading": "Loading the settings of {name}…",
  "live.empty": "{name} has no settings.",
  "live.unreachable":
    "{name} is running, but its settings cannot be reached. Versions without the tuxhub interface can only be started and quit here.",
  "live.advanced": "Advanced",
  "live.reset": "Reset {label}",
  "live.actionDone": "{label}: done",
  "live.protocol": "This version of {name} does not match tuxhub.",
  "live.protocolHint": "Update {name} or tuxhub to change its settings here.",
  "live.unsupported": "Not supported by this version of tuxhub.",
  "live.invalid": "{count} entries from {name} could not be shown.",
  "live.invalidDetails": "Details for developers",
  "live.error.timeout": "{name} did not answer in time.",
  "live.error.invalid-answer": "The answer of {name} was invalid.",
  "live.error.closed": "{name} was closed.",
  "live.error.not-running": "{name} is not running.",
  "live.error.protocol": "This version of {name} does not match tuxhub.",
  "live.error.unknown": "{name} no longer offers this.",
  "live.error.range": "Allowed: {min} to {max}.",
  "live.error.option": "Choose one of the options.",
  "live.error.color": "Enter a color such as #ff8800.",
  "live.error.length": "At most {max} characters.",
  "live.error.type": "This value does not fit.",
  "live.error.disabled": "This setting cannot be changed.",

  "workshop.title": "Workshop",
  "workshop.subtitle": "Curated tools for gaming on Linux.",
  "workshop.offline": "Offline list",
  "workshop.search": "Search",
  "workshop.all": "All",
  "workshop.loading": "Loading workshop…",
  "workshop.noResults": "No matching tools.",
  "workshop.showAll": "Show all tools",
  "workshop.loadFailed": "The workshop could not be loaded.",
  "workshop.categories": "Categories",
  "workshop.installed": "Installed",
  "workshop.open": "Open",
  "workshop.install": "Install",
  "workshop.installing": "Installing…",
  "workshop.installedToast": "{name} installed",
  "workshop.close": "Close",

  "progress.resolving": "Looking up the latest release…",
  "progress.downloading": "Downloading {file} ({mb} MB)",
  "progress.extracting": "Extracting…",
  "progress.flathub": "Setting up Flathub…",
  "progress.flatpak": "Installing {id}. This may take a while…",
  "progress.package": "Installing the package. Enter your password when asked…",

  "hub.title": "Settings",
  "hub.general": "General",
  "hub.autostart": "Start when I log in",
  "hub.startMinimized": "Start in the system tray only",
  "hub.closeToTray": "Keep running in the system tray when closed, so hotkeys keep working",
  "hub.checkUpdates": "Check for tool updates automatically (every 6 hours)",
  "hub.checkNow": "Check for updates now",
  "hub.checking": "Checking…",
  "hub.updatesFound": "{count} update(s) found",
  "hub.allCurrent": "All tools are up to date",
  "hub.language": "Language",
  "hub.languageAuto": "System default",
  "hub.hotkeys": "Hotkeys",
  "hub.hotkeysHint":
    "Global hotkeys are provided by the desktop portal. You can change the keys in your system settings.",
  "hub.hotkeysLoading": "Loading…",
  "hub.hotkeysNone": "No hotkeys registered.",
  "hub.hotkeysUnassigned": "Not assigned",
  "hub.hotkeysConfigure": "Configure hotkeys…",
  "hub.dev": "Developer mode: link a local tool",
  "hub.devHint":
    "Select a folder that contains a tuxhub.toml with an id and a name. tuxhub uses the folder directly, which is convenient while developing a tool.",
  "hub.devLink": "Link",
  "hub.devLinked": "Local tool linked",
  "hub.about": "About",
  "hub.aboutText": "tuxhub {version}. A modular hub for gaming on Linux.",
  "hub.selfUpdate": "tuxhub updates",
  "hub.selfCheck": "Check for tuxhub updates",
  "hub.selfCurrent": "You are running the latest version.",
  "hub.selfDev": "Updates are not available in development builds.",

  "selfupdate.available": "tuxhub {version} is available.",
  "selfupdate.install": "Update now",
  "selfupdate.later": "Later",
  "selfupdate.downloading": "Downloading update… {percent}%",
  "selfupdate.installing": "Installing update… If the system asks for your password, confirm there.",
  "selfupdate.failed": "Update failed: {error}",
  "selfupdate.notes": "Release notes",
  "selfupdate.done": "tuxhub was updated to {version}.",
  "selfupdate.whatsNew": "What's new",
  "selfupdate.section.Added": "New",
  "selfupdate.section.Changed": "Changed",
  "selfupdate.section.Fixed": "Fixed",
  "selfupdate.section.Removed": "Removed",
  "selfupdate.section.Security": "Security",
  "selfupdate.section.Deprecated": "Deprecated",
} as const;

export type Key = keyof typeof en;

const de: Record<Key, string> = {
  "nav.workshop": "Workshop",
  "nav.myTools": "Meine Tools",
  "nav.settings": "Einstellungen",
  "nav.empty": "Noch nichts installiert. Im Workshop findest du passende Tools.",
  "nav.updateOne": "1 Update installieren",
  "nav.updateMany": "{count} Updates installieren",
  "nav.updating": "Aktualisiere…",
  "nav.updateTo": "Update verfügbar: {version}",
  "nav.active": "Aktiv",

  "common.retry": "Erneut versuchen",
  "common.cancel": "Abbrechen",
  "common.dismiss": "Schließen",

  "category.input": "Eingabe",
  "category.compatibility": "Kompatibilität",
  "category.overlay": "Overlay",
  "category.launcher": "Launcher",
  "category.creative": "Kreativ",
  "category.local": "Lokal",
  "category.other": "Sonstiges",

  "source.github-release": "GitHub-Release",
  "source.flatpak": "Flatpak",
  "source.local": "Lokal",
  "source.external": "Separat installiert",
  "source.system-package": "Systempaket",

  "level.0": "Eigenständig",
  "level.1": "Web-Oberfläche",
  "level.2": "Integriert",
  "level.3": "Live",

  "tool.running": "Läuft",
  "tool.openWindow": "Eigenes Fenster öffnen",
  "tool.started": "{name} gestartet",
  "tool.checkUpdates": "Nach Updates suchen",
  "tool.isUpToDate": "{name} ist aktuell",
  "tool.updateTo": "Auf {version} aktualisieren",
  "tool.updatedTo": "Auf {version} aktualisiert",
  "tool.stopProcess": "Prozess beenden",
  "tool.reconnect": "Neu verbinden",
  "tool.ownWindow": "Im eigenen Fenster",
  "tool.uninstall": "Deinstallieren",
  "tool.unlink": "Verknüpfung lösen",
  "tool.confirmUninstall": "{name} deinstallieren?",
  "tool.confirmUnlink": "Verknüpfung zu {name} entfernen?",
  "tool.uninstallHint": "Das Tool, seine Einstellungen und Hotkeys werden aus tuxhub entfernt.",
  "tool.unlinkHint": "Die Einstellungen werden aus tuxhub entfernt. Der Ordner selbst wird nicht gelöscht.",
  "tool.removed": "{name} entfernt",
  "tool.noLauncher": "Für dieses Tool wurde keine ausführbare Datei gefunden.",
  "tool.unknownOp": "Unbekannte Operation: {op}",
  "tool.more": "Weitere Aktionen",

  "embed.start": "In tuxhub starten",
  "embed.hint": "Die Anwendung läuft innerhalb von tuxhub. Für 3D-Spiele ist „Im eigenen Fenster“ besser geeignet.",
  "embed.starting": "{name} wird gestartet…",
  "embed.stopping": "{name} wird beendet…",
  "embed.lost": "Verbindung zu {name} verloren",
  "embed.startedWindow": "{name} im eigenen Fenster geöffnet",
  "embed.missing": "Für die Anzeige innerhalb von tuxhub wird {dep} benötigt. Installation über die Paketverwaltung:",

  "app.start": "Starten",
  "app.quit": "Beenden",
  "app.running": "{name} läuft.",
  "app.stopped": "{name} läuft nicht.",
  "app.quitDone": "{name} beendet",
  "app.startToChange": "Starte {name}, um die Einstellungen zu ändern.",
  "app.hint": "{name} läuft in einem eigenen Fenster auf dem Desktop, auch wenn tuxhub geschlossen ist.",

  "web.checking": "Web-Oberfläche wird geprüft…",
  "web.unreachable": "Die Web-Oberfläche antwortet nicht. Läuft {name}?",
  "web.start": "{name} starten",
  "web.autoAppear": "Sie erscheint hier, sobald sie erreichbar ist.",
  "web.startedWaiting": "{name} gestartet. Warte auf die Web-Oberfläche…",
  "web.wsHint": "Live-Funktionen über WebSockets funktionieren unter Umständen nur im eigenen Fenster.",

  "readme.loading": "README wird geladen…",
  "readme.unavailable": "README nicht verfügbar: {error}",

  "settings.saved": "Gespeichert",
  "settings.saving": "Wird gespeichert…",

  "live.loading": "Einstellungen von {name} werden geladen…",
  "live.empty": "{name} hat keine Einstellungen.",
  "live.unreachable":
    "{name} läuft, aber seine Einstellungen sind nicht erreichbar. Versionen ohne tuxhub-Schnittstelle lassen sich hier nur starten und beenden.",
  "live.advanced": "Erweitert",
  "live.reset": "{label} zurücksetzen",
  "live.actionDone": "{label}: erledigt",
  "live.protocol": "Diese Version von {name} passt nicht zu tuxhub.",
  "live.protocolHint": "Aktualisiere {name} oder tuxhub, um die Einstellungen hier zu ändern.",
  "live.unsupported": "Wird von dieser tuxhub-Version nicht unterstützt.",
  "live.invalid": "{count} Einträge von {name} können nicht angezeigt werden.",
  "live.invalidDetails": "Details für Entwickler",
  "live.error.timeout": "{name} hat nicht rechtzeitig geantwortet.",
  "live.error.invalid-answer": "Antwort der App ungültig.",
  "live.error.closed": "{name} wurde beendet.",
  "live.error.not-running": "{name} läuft nicht.",
  "live.error.protocol": "Diese Version von {name} passt nicht zu tuxhub.",
  "live.error.unknown": "{name} bietet das nicht mehr an.",
  "live.error.range": "Erlaubt: {min} bis {max}.",
  "live.error.option": "Wähle eine der Möglichkeiten.",
  "live.error.color": "Gib eine Farbe wie #ff8800 ein.",
  "live.error.length": "Höchstens {max} Zeichen.",
  "live.error.type": "Dieser Wert passt nicht.",
  "live.error.disabled": "Diese Einstellung lässt sich nicht ändern.",

  "workshop.title": "Workshop",
  "workshop.subtitle": "Ausgewählte Tools für Gaming unter Linux.",
  "workshop.offline": "Offline-Liste",
  "workshop.search": "Suchen",
  "workshop.all": "Alle",
  "workshop.loading": "Workshop wird geladen…",
  "workshop.noResults": "Keine passenden Tools.",
  "workshop.showAll": "Alle Tools anzeigen",
  "workshop.loadFailed": "Der Workshop konnte nicht geladen werden.",
  "workshop.categories": "Kategorien",
  "workshop.installed": "Installiert",
  "workshop.open": "Öffnen",
  "workshop.install": "Installieren",
  "workshop.installing": "Wird installiert…",
  "workshop.installedToast": "{name} installiert",
  "workshop.close": "Schließen",

  "progress.resolving": "Neuestes Release wird gesucht…",
  "progress.downloading": "{file} wird geladen ({mb} MB)",
  "progress.extracting": "Wird entpackt…",
  "progress.flathub": "Flathub wird eingerichtet…",
  "progress.flatpak": "{id} wird installiert. Das kann etwas dauern…",
  "progress.package": "Paket wird installiert. Gib dein Passwort ein, wenn du gefragt wirst…",

  "hub.title": "Einstellungen",
  "hub.general": "Allgemein",
  "hub.autostart": "Beim Anmelden starten",
  "hub.startMinimized": "Dabei nur im Infobereich starten",
  "hub.closeToTray": "Beim Schließen im Infobereich weiterlaufen, damit Hotkeys funktionieren",
  "hub.checkUpdates": "Automatisch nach Tool-Updates suchen (alle 6 Stunden)",
  "hub.checkNow": "Jetzt nach Updates suchen",
  "hub.checking": "Suche läuft…",
  "hub.updatesFound": "{count} Update(s) gefunden",
  "hub.allCurrent": "Alle Tools sind aktuell",
  "hub.language": "Sprache",
  "hub.languageAuto": "Systemsprache",
  "hub.hotkeys": "Hotkeys",
  "hub.hotkeysHint":
    "Globale Tastenkürzel werden über das Desktop-Portal bereitgestellt. Die Tasten lassen sich in den Systemeinstellungen ändern.",
  "hub.hotkeysLoading": "Wird geladen…",
  "hub.hotkeysNone": "Keine Hotkeys registriert.",
  "hub.hotkeysUnassigned": "Nicht belegt",
  "hub.hotkeysConfigure": "Hotkeys festlegen…",
  "hub.dev": "Entwicklermodus: lokales Tool verknüpfen",
  "hub.devHint":
    "Ordner mit einer tuxhub.toml (inklusive id und name). tuxhub verwendet den Ordner direkt, praktisch während der Entwicklung eines Tools.",
  "hub.devLink": "Verknüpfen",
  "hub.devLinked": "Lokales Tool verknüpft",
  "hub.about": "Über",
  "hub.aboutText": "tuxhub {version}. Ein modularer Hub für Gaming unter Linux.",
  "hub.selfUpdate": "tuxhub-Updates",
  "hub.selfCheck": "Nach tuxhub-Updates suchen",
  "hub.selfCurrent": "Du verwendest die neueste Version.",
  "hub.selfDev": "In Entwicklungsversionen sind Updates nicht verfügbar.",

  "selfupdate.available": "tuxhub {version} ist verfügbar.",
  "selfupdate.install": "Jetzt aktualisieren",
  "selfupdate.later": "Später",
  "selfupdate.downloading": "Update wird geladen… {percent} %",
  "selfupdate.installing": "Update wird installiert… Fragt das System nach deinem Passwort, bestätige dort.",
  "selfupdate.failed": "Update fehlgeschlagen: {error}",
  "selfupdate.notes": "Versionshinweise",
  "selfupdate.done": "tuxhub wurde auf {version} aktualisiert.",
  "selfupdate.whatsNew": "Was ist neu?",
  "selfupdate.section.Added": "Neu",
  "selfupdate.section.Changed": "Geändert",
  "selfupdate.section.Fixed": "Behoben",
  "selfupdate.section.Removed": "Entfernt",
  "selfupdate.section.Security": "Sicherheit",
  "selfupdate.section.Deprecated": "Veraltet",
};

export type Locale = "en" | "de";
export const LOCALES: { id: Locale; label: string }[] = [
  { id: "en", label: "English" },
  { id: "de", label: "Deutsch" },
];

const dictionaries: Record<Locale, Record<Key, string>> = { en, de };
const STORAGE_KEY = "tuxhub.locale";

function systemLocale(): Locale {
  return navigator.language?.toLowerCase().startsWith("de") ? "de" : "en";
}

function storedLocale(): Locale | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return v === "en" || v === "de" ? v : null;
  } catch {
    return null;
  }
}

export const i18n = $state({
  /** Explicit choice, or null to follow the system language. */
  choice: storedLocale() as Locale | null,
  locale: storedLocale() ?? systemLocale(),
});

export function setLocale(choice: Locale | null) {
  i18n.choice = choice;
  i18n.locale = choice ?? systemLocale();
  document.documentElement.lang = i18n.locale;
  try {
    if (choice) localStorage.setItem(STORAGE_KEY, choice);
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Storage may be unavailable; the choice then lasts for this session only.
  }
}

/** Translates `key` into the current language and fills `{placeholders}`. */
export function t(key: Key, vars?: Record<string, string | number>): string {
  let s: string = dictionaries[i18n.locale][key] ?? en[key];
  if (vars) {
    for (const [k, v] of Object.entries(vars)) s = s.replaceAll(`{${k}}`, String(v));
  }
  return s;
}

/** Translates a category id, falling back to the raw value for unknown ones. */
export function category(id: string | undefined): string {
  if (!id) return t("category.other");
  const key = `category.${id.toLowerCase()}` as Key;
  return key in en ? t(key) : id;
}
