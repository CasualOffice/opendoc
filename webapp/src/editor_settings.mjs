// Settings: theme, accent, reviewer identity, autosave and proofing switches,
// persisted, and the dialog that edits them; then localisation's start. Moved
// out of `main.js` unchanged (docs/177); it reaches the rest of the editor
// only through the context `ed`.
import { refreshStylePreviews } from "./style_preview.mjs";
import { registerModal } from "./modal.mjs";
import { t } from "./i18n.mjs";
import { startLocalisation } from "./locale_boot.mjs";
import { localizeShortcutGlyphs } from "./shortcut_labels.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { registerPopover } from "./popover_manager.mjs";
import { DEFAULT_SETTINGS } from "./settings_defaults.mjs";
import { setPaneRenderer, showSettingsPane } from "./file_pane.mjs";
import { applyAppearance, brandAccent, reflectAppearance } from "./appearance.mjs";
import { activeAuthor, hostIdentity, reflectAuthorIdentity } from "./host_identity.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createSettings(ed) {
  const settingsPanel = document.getElementById("settingsPanel");
  const themeSeg = document.getElementById("themeSeg");
  const accentSwatches = document.getElementById("accentSwatches");
  const accentCustom = document.getElementById("accentCustom");
  const settingsReset = document.getElementById("settingsReset");
  const settingsClose = document.getElementById("settingsClose");
  const languageSelect = document.getElementById("languageSelect");
  const authorNameInput = document.getElementById("authorName");
  const authorInitialsInput = document.getElementById("authorInitials");

  // Who owns the accent: the deployment's `brand.json`, or the visitor. Read once,
  // before the first `applySettings()`, because a brand resolved after the first
  // paint was never resolved before it.
  const ACCENT_PIN = brandAccent(document.documentElement);

  const authorIdentityReason = document.getElementById("authorIdentityReason");

  /** Who is writing, onto the engine: the host identity when there is one (it is
   *  then the ONLY identity, `docs/175` §3.1), else the Settings name, else "You". */
  function applyActiveAuthorToDocument() {
    if (!ed.doc) return;
    const who = activeAuthor(ed.settings);
    ed.doc.setActiveAuthor(who.name, who.initials, who.id);
    // `people.xml` presenceInfo is OPT-IN (§6.2): only a host's `persist` puts an
    // id into a saved file, and without it nothing is called at all.
    if (who.persist && typeof ed.doc.setAuthorPersistence === "function") {
      ed.doc.setAuthorPersistence(who.persist.providerId, who.persist.userId);
    }
  }
  /** The two author fields: the visitor's, or the host's and disabled with why. */
  function reflectAuthorFields() {
    reflectAuthorIdentity({ nameInput: authorNameInput, initialsInput: authorInitialsInput,
      reason: authorIdentityReason, settings: ed.settings });
  }
  // A host identity arrives by message after boot (`docs/175` §3.2).
  hostIdentity.onChange(() => applySettings());

  /** Applies the current settings to the document root + reflects them in the panel. */
  function applySettings() {
    applyAppearance({
      root: document.documentElement,
      theme: ed.settings.theme,
      accent: ed.settings.accent,
      pin: ACCENT_PIN,
    });
    // The gallery is built per document, but its legibility decisions were made
    // against whatever palette was live at the time.
    refreshStylePreviews();

    reflectAppearance({
      themeGroup,
      swatches: accentSwatches,
      custom: accentCustom,
      settings: ed.settings,
      pin: ACCENT_PIN,
      reason: t("branding.themeSetByHost"),
    });
    reflectAuthorFields();
    if (ed.autosaveToggle) {
      ed.autosaveToggle.checked = ed.settings.autosave !== false;
      // A control that cannot do anything says why, rather than sitting there
      // looking operable (SKILL.md §10). In a host iframe autosave is off by
      // policy, not by preference.
      ed.autosaveToggle.disabled = !ed.AUTOSAVE_ALLOWED_HERE;
      ed.autosaveToggle.title = ed.AUTOSAVE_ALLOWED_HERE
        ? ""
        : "Autosave is off in an embedded editor — the page that embeds it owns saving.";
    }
    if (ed.draftsClearBtn) ed.draftsClearBtn.disabled = !ed.AUTOSAVE_ALLOWED_HERE;
    // Version history rides on the autosave switch (ADR-038 / `docs/139` §18
    // question 2: one switch must not promise what the other has stopped doing),
    // so both switches and the retention numbers change what this entry says.
    ed.versionHistory.reflect();
    if (ed.spellCheckToggle) ed.spellCheckToggle.checked = ed.settings.spellCheck !== false;
    if (ed.grammarCheckToggle) ed.grammarCheckToggle.checked = ed.settings.grammarCheck !== false;
    applyActiveAuthorToDocument();
  }

  ed.autosaveToggle?.addEventListener("change", () => {
    ed.settings.autosave = ed.autosaveToggle.checked;
    ed.saveSettings();
    if (ed.settings.autosave) {
      ed.setDraftStatus("");
      ed.noteDraftDirty();
      void ed.queueDraftWork(() => ed.refreshDraftOffers());
    } else {
      // Off means off: the bytes go too, or "off" would only mean "stop adding".
      void ed.clearAllDrafts({ confirm: false });
      ed.setDraftStatus("Autosave off", "off", "Autosave is off. Turn it back on in Settings ▸ Autosave.");
    }
    // Version history rides this switch (ADR-038). Stored VERSIONS are left alone:
    // `clear()` above empties the draft slots only, and deleting a timeline as a
    // side effect of a switch is a destruction nobody asked for.
    ed.versionHistory.reflect();
  });
  ed.draftsClearBtn?.addEventListener("click", () => void ed.clearAllDrafts());
  ed.spellCheckToggle?.addEventListener("change", () => ed.setSpellCheckEnabled(ed.spellCheckToggle.checked));
  ed.grammarCheckToggle?.addEventListener("change", () => ed.setGrammarCheckEnabled(ed.grammarCheckToggle.checked));

  const themeGroup = bindRadioGroup(themeSeg, {
    attr: "data-theme",
    onSelect: (theme) => {
      ed.settings.theme = theme;
      ed.saveSettings();
      applySettings();
    },
  });
  accentSwatches.addEventListener("click", (e) => {
    const b = e.target.closest(".acc[data-accent]");
    if (!b) return;
    ed.settings.accent = b.dataset.accent;
    ed.saveSettings();
    applySettings();
  });
  accentCustom.addEventListener("input", () => {
    ed.settings.accent = accentCustom.value;
    ed.saveSettings();
    applySettings();
  });
  authorNameInput.addEventListener("input", () => {
    ed.settings.authorName = authorNameInput.value;
    ed.saveSettings();
    applyActiveAuthorToDocument();
  });
  authorInitialsInput.addEventListener("input", () => {
    ed.settings.authorInitials = authorInitialsInput.value;
    ed.saveSettings();
    applyActiveAuthorToDocument();
  });
  settingsReset.addEventListener("click", () => {
    ed.settings = { ...DEFAULT_SETTINGS };
    ed.saveSettings();
    applySettings();
  });

  // Settings is a MODAL now, registered like every other dialog here. It was an
  // anchored popover with its own Escape handler and its own pointerdown light
  // dismiss — the hand-rolled dismissal `modal.mjs` exists to end — and it was
  // taller than the window, so scrolling it scrolled the document behind it. The
  // owner's report: "that panel doesn't make any sense now .. see dialog
  // instead". Registering it means Escape, the backdrop, the close button and
  // focus restoration are one path, Tab cannot walk out into the chrome behind
  // the scrim, and application shortcuts stop firing behind it.
  const settingsModal = registerModal(settingsPanel, {
    initialFocus: () => themeGroup.selected(),
    fallbackFocus: () => ed.settingsBtn,
  });

  function toggleSettings(open) {
    const show = open ?? !settingsModal.isOpen;
    if (show === settingsModal.isOpen) return;
    ed.settingsBtn.setAttribute("aria-expanded", String(show));
    if (show) settingsModal.open();
    else settingsModal.close();
  }

  ed.settingsBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    // The Settings DIALOG and the Settings PANE are the same element, and while
    // the File page is open it is parented INSIDE the page — opening the dialog
    // would raise a half-dialog out of a pane. One Settings, so the gear goes to
    // wherever it currently lives.
    if (showSettingsPane()) {
      document.querySelector('#filePageBody [data-file-pane="settings"]')?.focus();
      return;
    }
    toggleSettings();
  });
  settingsClose.addEventListener("click", () => toggleSettings(false));
  document.getElementById("settingsDone")?.addEventListener("click", () => toggleSettings(false));

  // Settings pages (approved design D3, Word Options' left navigation): one
  // topic on screen at a time instead of every section in two long columns.
  // Tabs with arrow-key movement; the chosen page is remembered for the
  // session. Works in the dialog and in the File page's Settings pane, which
  // are the same element.
  const settingsBody = settingsPanel.querySelector(".settings-body");
  const settingsTabs = [...settingsPanel.querySelectorAll("[data-settings-tab]")];
  function showSettingsPage(page, focus = false) {
    if (!settingsBody || !settingsTabs.length) return;
    const known = settingsTabs.some((tab) => tab.dataset.settingsTab === page);
    const current = known ? page : settingsTabs[0].dataset.settingsTab;
    settingsBody.dataset.settingsPage = current;
    for (const tab of settingsTabs) {
      const on = tab.dataset.settingsTab === current;
      tab.setAttribute("aria-selected", String(on));
      tab.tabIndex = on ? 0 : -1;
      if (on && focus) tab.focus();
    }
    settingsPanel.querySelector(".settings-pages")?.scrollTo?.(0, 0);
  }
  for (const tab of settingsTabs) {
    tab.addEventListener("click", () => showSettingsPage(tab.dataset.settingsTab));
    tab.addEventListener("keydown", (e) => {
      const i = settingsTabs.indexOf(tab);
      const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
      if (step) {
        e.preventDefault();
        showSettingsPage(settingsTabs[(i + step + settingsTabs.length) % settingsTabs.length].dataset.settingsTab, true);
      } else if (e.key === "Home" || e.key === "End") {
        e.preventDefault();
        showSettingsPage(settingsTabs[e.key === "Home" ? 0 : settingsTabs.length - 1].dataset.settingsTab, true);
      }
    });
  }
  showSettingsPage("general");
  applySettings();
  // After `applySettings`, so the picker is built against the settings that were
  // actually loaded, and awaited nowhere: the editor draws in English and the
  // chosen language relabels it when its catalogue lands.
  // Locale (docs/124). After `applySettings`, so the picker sees the settings
  // that loaded; awaited nowhere, so the editor draws in English and relabels
  // when the catalogue lands. The counts are the one surface markup cannot
  // carry, so re-rendering them is what this hands over.
  setPaneRenderer(() => ed.renderFilePage());
  void startLocalisation({
    select: languageSelect,
    // The footer control is a POPOVER, on the same manager the zoom presets use,
    // so the two footer controls dismiss, restore focus and stay mutually
    // exclusive by one implementation rather than two. The module owns the rest
    // of that control's DOM already, so it looks the elements up itself.
    registerPopover,
    settings: ed.settings,
    saveSettings: ed.saveSettings,
    onLocalised: () => {
      // Every relabel re-introduces ⌘ from the catalogue (`105` UX-009, #599).
      localizeShortcutGlyphs(document.body, ed.EDITOR_KEYBOARD_PLATFORM);
      ed.reflowView.setEnabled(); // the width button's label is the STEP's, not markup's
      ed.pdfCommands?.chip?.reflect(); // the PDF layout chip names its layout in a sentence
      if (ed.doc) ed.updateStats();
      // The timeline's rows, day headings and disclosure are all script-built from
      // locale-shaped values, and the View entry's disabled reason is a sentence —
      // so this panel is one of the surfaces a relabel has to reach.
      ed.versionHistory.reflect();
      // The disabled fields' title is the reason's sentence, which just relabelled.
      reflectAuthorFields();
    },
  });

  return { applyActiveAuthorToDocument, toggleSettings };
}
