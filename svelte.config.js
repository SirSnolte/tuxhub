// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  compilerOptions: {
    // Web Awesome elements such as <wa-button> wrap a native, keyboard
    // accessible control in their shadow root. Svelte cannot see that and
    // asks for a role and key handlers, which would duplicate the control for
    // assistive technology. All other accessibility warnings stay active.
    warningFilter: (w) =>
      !(
        (w.code === "a11y_click_events_have_key_events" || w.code === "a11y_no_static_element_interactions") &&
        /`<wa-[a-z-]+>`/.test(w.message)
      ),
  },
  kit: {
    adapter: adapter({
      fallback: "index.html",
    }),
  },
};

export default config;
