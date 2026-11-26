// The state both windows share: what the backend sends, and which requests
// the user opened in full. Each window has its own copy of this module.
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";
import type { View } from "./deck";

export const deck = $state<{
  view: View | null;
  /// Requests whose full text the user opened: only those may be allowed.
  expanded: Record<string, boolean>;
  error: string | null;
}>({ view: null, expanded: {}, error: null });

/// Follow the backend; returns the function that stops it.
export function follow(): () => void {
  const stop = listen<View>("deck://state", (e) => {
    deck.view = e.payload;
  });
  // "View" in a notification: the panel opens with the full text.
  const view = listen<string>("deck://view", (e) => {
    deck.expanded[e.payload] = true;
  });
  // What was sent before this page listened.
  invoke<View | null>("current").then((v) => {
    if (v && deck.view === null) deck.view = v;
  });
  return () => {
    stop.then((f) => f());
    view.then((f) => f());
  };
}

export async function answer(id: string, allow: boolean) {
  try {
    await invoke("answer", { requestId: id, allow });
    deck.error = null;
  } catch (e) {
    deck.error = t("Error: {error}", { error: String(e) });
  }
}

export function openMain() {
  invoke("open_main").catch(() => {});
}
