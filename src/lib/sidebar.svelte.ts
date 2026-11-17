// Pinning, unpinning and ordering the sidebar. The interface changes at once;
// when the backend refuses, it is turned back and the error shown. Each change
// offers "Undo" for five seconds and is read out to screen readers.
import { api, errText } from "./api";
import { t } from "./i18n.svelte";
import { insertAt } from "./workshop-model";
import { announce, toast } from "./state.svelte";

export const sidebar = $state({
  /** Ids of the pinned tools, top first. */
  order: [] as string[],
  /** Ids that were just pinned (they flash once). */
  fresh: [] as string[],
});

/** Reads the sidebar again (the page's tool list). */
let refresh: () => Promise<void> = async () => {};

export function connectSidebar(reload: () => Promise<void>) {
  refresh = reload;
}

function say(text: string, undo?: () => void) {
  announce(text);
  toast(text, "info", undo);
}

/** Pins tools or start menu apps (`desktop:<id>`) at `at`; `name` names the
 * first one in the message. Returns the tool ids that are new in the sidebar. */
export async function pin(ids: string[], at: number | null, name: string): Promise<string[]> {
  const before = [...sidebar.order];
  sidebar.order = insertAt(before, ids.filter((id) => !id.startsWith("desktop:")), at);
  try {
    const order = await api.sidebarPin(ids, at);
    const added = order.filter((id) => !before.includes(id));
    sidebar.order = order;
    sidebar.fresh = added;
    setTimeout(() => (sidebar.fresh = []), 700);
    await refresh();
    if (added.length > 0) {
      const undo = async () => {
        for (const id of added) await unpinQuietly(id);
      };
      say(added.length > 1 ? t("ws.pinnedMany", { count: added.length }) : t("ws.pinned", { name }), undo);
    }
    return added;
  } catch (e) {
    sidebar.order = before;
    toast(`${name}: ${errText(e)}`);
    return [];
  }
}

async function unpinQuietly(id: string) {
  try {
    sidebar.order = await api.sidebarUnpin(id);
    await refresh();
  } catch (e) {
    toast(errText(e));
  }
}

/** Takes a tool out of the sidebar. `again` is how to pin it back for
 * "Undo" (an app added from the start menu loses its record). */
export async function unpin(id: string, name: string, again: string = id): Promise<void> {
  const before = [...sidebar.order];
  const at = before.indexOf(id);
  sidebar.order = before.filter((x) => x !== id);
  try {
    sidebar.order = await api.sidebarUnpin(id);
    await refresh();
    say(t("ws.unpinned", { name }), async () => {
      try {
        sidebar.order = await api.sidebarPin([again], at < 0 ? null : at);
        await refresh();
      } catch (e) {
        toast(errText(e));
      }
    });
  } catch (e) {
    sidebar.order = before;
    toast(`${name}: ${errText(e)}`);
  }
}

/** Moves a pinned tool to place `to` (0 = top). */
export async function move(id: string, to: number, name: string, withUndo = true): Promise<void> {
  const before = [...sidebar.order];
  const from = before.indexOf(id);
  if (from < 0) return;
  const target = Math.max(0, Math.min(to, before.length - 1));
  if (target === from) return;
  const order = before.filter((x) => x !== id);
  order.splice(target, 0, id);
  sidebar.order = order;
  try {
    await api.sidebarSetOrder(order);
    await refresh();
    const text = t("ws.moved", { name, place: target + 1 });
    if (withUndo) {
      say(text, async () => {
        sidebar.order = before;
        try {
          await api.sidebarSetOrder(before);
          await refresh();
        } catch (e) {
          toast(errText(e));
        }
      });
    } else {
      announce(text);
    }
  } catch (e) {
    sidebar.order = before;
    toast(`${name}: ${errText(e)}`);
  }
}
