// One signal for "Settings is showing, or just changed language" (`docs/184` D3).
//
// Parts of Settings are script-built, not markup: the add-in rows, the add-in
// catalogue's status line, the AI provider form and its summary sentence. They
// used to refresh on a click on the gear button, so every other way in (the
// palette, the status bar's unit chip, the File page's Settings pane, the AI
// entry points) showed whatever the last gear click left, and a language chosen
// on Settings' own General page left those rows in the previous language until
// the gear was clicked again. The Settings module now announces both moments on
// the panel element itself, from the modal's `onOpen`, the File page's pane
// preparation and the relabel hook, and each script-built part listens here.
//
// reason "open"    — Settings was just shown, as a dialog or as the File pane.
// reason "relabel" — the language in force changed; re-render, keep the input.

export const SETTINGS_REFRESH = "settings-refresh";

/** Tells every script-built part of Settings to refresh itself. */
export function announceSettings(reason, doc = globalThis.document) {
  doc?.getElementById("settingsPanel")?.dispatchEvent(new CustomEvent(SETTINGS_REFRESH, { detail: { reason } }));
}

/** Runs `handler(reason)` on each announcement, from any route. */
export function onSettingsRefresh(handler, doc = globalThis.document) {
  doc?.getElementById("settingsPanel")?.addEventListener(SETTINGS_REFRESH, (event) => handler(event.detail?.reason ?? "open"));
}
