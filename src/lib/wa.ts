// Web Awesome components used by the interface, with the tuxhub theme that
// tools share (ui-kit/theme.css).
import "@awesome.me/webawesome/dist/styles/themes/default.css";
import "@awesome.me/webawesome/dist/styles/utilities.css";
import "../../ui-kit/theme.css";
import { registerTranslation, unregisterIconLibrary } from "@awesome.me/webawesome";
import de from "@awesome.me/webawesome/dist/translations/de.js";
import "@awesome.me/webawesome/dist/components/button/button.js";
import "@awesome.me/webawesome/dist/components/callout/callout.js";
import "@awesome.me/webawesome/dist/components/card/card.js";
import "@awesome.me/webawesome/dist/components/details/details.js";
import "@awesome.me/webawesome/dist/components/dialog/dialog.js";
import "@awesome.me/webawesome/dist/components/dropdown/dropdown.js";
import "@awesome.me/webawesome/dist/components/dropdown-item/dropdown-item.js";
import "@awesome.me/webawesome/dist/components/divider/divider.js";
import "@awesome.me/webawesome/dist/components/icon/icon.js";
import "@awesome.me/webawesome/dist/components/input/input.js";
import "@awesome.me/webawesome/dist/components/option/option.js";
import "@awesome.me/webawesome/dist/components/progress-bar/progress-bar.js";
import "@awesome.me/webawesome/dist/components/radio/radio.js";
import "@awesome.me/webawesome/dist/components/radio-group/radio-group.js";
import "@awesome.me/webawesome/dist/components/select/select.js";
import "@awesome.me/webawesome/dist/components/skeleton/skeleton.js";
import "@awesome.me/webawesome/dist/components/spinner/spinner.js";
import "@awesome.me/webawesome/dist/components/switch/switch.js";
import "@awesome.me/webawesome/dist/components/tag/tag.js";

registerTranslation(de);
// The default icon library loads icons from a CDN; only the built-in system
// icons and Lucide are used.
unregisterIconLibrary("default");

/** Follows the light or dark appearance of the desktop. */
export function followColorScheme() {
  const media = matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    document.documentElement.classList.toggle("wa-dark", media.matches);
    document.documentElement.classList.toggle("wa-light", !media.matches);
  };
  apply();
  media.addEventListener("change", apply);
}

/** Value of the Web Awesome control that fired `e` (input, select, radio group). */
export function valueOf(e: Event): string {
  return String((e.currentTarget as unknown as { value: unknown }).value ?? "");
}

/** Checked state of the wa-switch or wa-checkbox that fired `e`. */
export function checkedOf(e: Event): boolean {
  return Boolean((e.currentTarget as unknown as { checked: unknown }).checked);
}
