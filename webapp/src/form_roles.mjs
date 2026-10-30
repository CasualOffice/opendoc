// Forms: Protect form, and the roles that decide who fills which field
// (`docs/153` review.protect-form, shell.form-roles; `docs/109` RM-34).
//
// PROTECT FORM is ONLYOFFICE's `btnProtectForm` and Word's Restrict Editing ▸
// "Filling in forms" in one press: the document becomes forms-protected, so only
// its content controls and legacy form fields take input. It is the SAME
// restriction the Restrict Editing dialog installs (`document_protection.mjs`,
// ADR-059), written through the same engine call, so the two can never disagree
// about whether a form is protected. A restriction that carries a password is
// not toggled from here — lifting it needs the password, which is the dialog's
// to ask for — so the press opens that dialog instead.
//
// ROLES are ONLYOFFICE's roles manager (`btnManager` / "Manage roles" on their
// Forms tab): named, coloured roles, each control assigned to one, and "View
// form as" a role so a filler sees and fills only theirs. OOXML has nowhere to
// keep a role (`crates/casual-doc-wasm/src/form_roles.rs` says so in full), so
// they live in the engine's sidecar keyed by each control's `w:id`, and this
// module keeps that sidecar in this browser under the file's name and hands it
// to the engine when the file opens. They do NOT travel inside the .docx, and
// the dialog says so.
import { t } from "./i18n.mjs";
import { readPref, writePref } from "./prefs.mjs";
import { registerModal } from "./modal.mjs";

/** The storage key for a file's roles. */
export function rolesStorageKey(name) {
  return `opendoc.formRoles.v1:${String(name || "untitled")}`;
}

/**
 * The sidecar to hand the engine after the roles dialog's edits.
 *
 * Pure: `rows` are the dialog's rows (`{original, name, color}`), `previous`
 * the sidecar the dialog opened on. A renamed role keeps its controls (the
 * assignments follow the rename); a removed one releases them to anyone; the
 * reader stops filling as a role that no longer exists.
 *
 * @param {{roles?: Array<{name: string, color: string}>, assignments?: Record<string, string>, fillingAs?: string}} previous
 * @param {ReadonlyArray<{original?: string, name: string, color: string}>} rows
 * @param {string} fillingAs
 */
export function nextRoles(previous, rows, fillingAs) {
  const renamed = new Map();
  const roles = [];
  for (const row of rows) {
    const name = String(row.name ?? "").trim();
    if (!name) continue;
    if (row.original) renamed.set(row.original, name);
    roles.push({ name, color: String(row.color ?? "").replace(/^#/, "").toUpperCase() || "3355C4" });
  }
  const names = new Set(roles.map((role) => role.name));
  const assignments = {};
  for (const [control, role] of Object.entries(previous?.assignments ?? {})) {
    const next = renamed.get(role) ?? null;
    if (next && names.has(next)) assignments[control] = next;
  }
  const out = { roles, assignments };
  const fill = renamed.get(fillingAs) ?? fillingAs;
  if (fill && names.has(fill)) out.fillingAs = fill;
  return out;
}

/** Which controls the roles dialog lists for `filter`: `"*"` every control,
 *  `""` those assigned to no role (anyone's), else that role's. Pure. */
export function controlsForRole(controls, filter) {
  if (filter === "*") return controls.slice();
  return controls.filter((control) => (control.role ?? "") === filter);
}

/**
 * Wires Protect form and the roles dialog.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc
 * @param {() => string} io.fileName the open file's name, which keys the roles
 * @param {(thunk: () => unknown, options?: object) => Promise<boolean>} io.runEdit
 * @param {() => void} io.openRestrictEditing
 * @param {(node: string, offset: number) => void} io.select
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {() => string} io.participantRefusal
 * @param {() => void} io.changed after the roles change (the controls' state moved)
 * @param {() => Element|null} io.fallbackFocus
 * @param {(kind: string) => string} io.kindLabel
 */
export function createFormRoles(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("formRolesDialog");
  const list = el("formRolesList");
  const add = el("formRolesAdd");
  const fillAs = el("formRolesFillAs");
  const filter = el("formRolesFilter");
  const fields = el("formRolesFields");
  const save = el("formRolesSave");
  const cancel = el("formRolesCancel");
  const close = el("formRolesClose");
  let opened = null;
  let loadedFor = null;

  function read() {
    const doc = io.getDoc();
    if (!doc || typeof doc.formRoles !== "function") return { roles: [], assignments: {} };
    try {
      return JSON.parse(doc.formRoles());
    } catch {
      return { roles: [], assignments: {} };
    }
  }

  /** The roles, for the properties dialog's select. */
  const roles = () => read().roles ?? [];

  /** Hands a newly opened document the roles this browser kept for its file. */
  function reflect() {
    const doc = io.getDoc();
    if (!doc || doc === loadedFor) return;
    loadedFor = doc;
    const stored = readPref(rolesStorageKey(io.fileName()), "");
    if (!stored || typeof doc.setFormRoles !== "function") return;
    try {
      const parsed = JSON.parse(stored);
      delete parsed.fillingAs;
      doc.setFormRoles(JSON.stringify(parsed));
    } catch {
      // A stored sidecar this build cannot read is dropped rather than applied.
    }
  }

  function persist(sidecar) {
    const { fillingAs: _ignored, ...kept } = sidecar;
    writePref(rolesStorageKey(io.fileName()), JSON.stringify(kept));
  }

  // ---- Protect form ---------------------------------------------------------------
  /** Whether the document is forms-protected and the restriction is enforced. */
  function isProtected() {
    const doc = io.getDoc();
    if (!doc) return false;
    try {
      const protection = JSON.parse(doc.documentProtection() ?? "null");
      return protection?.edit === "forms" && protection?.enforcement === true;
    } catch {
      return false;
    }
  }

  async function protectForm() {
    const doc = io.getDoc();
    if (!doc) {
      io.setStatus(t("command.needsDocument"), "error");
      return;
    }
    const refusal = io.participantRefusal();
    if (refusal) {
      io.setStatus(refusal, "error");
      return;
    }
    let password = false;
    try {
      password = JSON.parse(doc.protectionPassword?.() ?? "null")?.present === true;
    } catch {
      password = false;
    }
    if (password) {
      // Changing a passworded restriction needs the password: the dialog asks.
      io.openRestrictEditing();
      return;
    }
    const on = isProtected();
    const landed = await io.runEdit(() => (on ? doc.setDocumentProtection(undefined, false, false) : doc.setDocumentProtection("forms", true, false)));
    if (landed) io.setStatus(on ? t("forms.unprotected") : t("forms.protected"));
  }

  // ---- The roles dialog -----------------------------------------------------------
  const modal = dialog
    ? registerModal(dialog, {
      initialFocus: () => list?.querySelector("input[type=text]") ?? add,
      fallbackFocus: io.fallbackFocus,
      defaultAction: () => void apply(),
    })
    : null;

  function roleRow(role = { name: "", color: "3355C4" }, original = "") {
    const row = document.createElement("li");
    row.className = "form-role-row";
    row.dataset.original = original;
    const color = document.createElement("input");
    color.type = "color";
    color.value = `#${String(role.color || "3355C4").toLowerCase()}`;
    color.className = "form-role-color";
    color.setAttribute("aria-label", t("formRoles.colour"));
    const name = document.createElement("input");
    name.type = "text";
    name.maxLength = 64;
    name.value = role.name;
    name.className = "form-role-name";
    name.setAttribute("aria-label", t("formRoles.name"));
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "dialog-button";
    remove.textContent = t("formRoles.remove");
    remove.addEventListener("click", () => {
      const next = row.nextElementSibling?.querySelector("input[type=text]") ?? add;
      row.remove();
      reflectFilter();
      next?.focus();
    });
    row.append(color, name, remove);
    return row;
  }

  function rows() {
    return [...(list?.querySelectorAll(".form-role-row") ?? [])].map((row) => ({
      original: row.dataset.original || "",
      name: row.querySelector(".form-role-name")?.value ?? "",
      color: row.querySelector(".form-role-color")?.value ?? "",
    }));
  }

  function fillSelect(select, options, value) {
    select.replaceChildren();
    for (const [optionValue, label] of options) {
      const option = document.createElement("option");
      option.value = optionValue;
      option.textContent = label;
      select.append(option);
    }
    select.value = options.some(([v]) => v === value) ? value : options[0]?.[0] ?? "";
  }

  /** The controls the filter names, each with a Go to. */
  function reflectFields() {
    if (!fields) return;
    const doc = io.getDoc();
    let controls = [];
    try {
      controls = JSON.parse(doc?.contentControls?.() ?? "[]");
    } catch {
      controls = [];
    }
    const shown = controlsForRole(controls, filter.value);
    fields.replaceChildren();
    if (!shown.length) {
      const empty = document.createElement("li");
      empty.className = "form-role-empty";
      empty.textContent = t("formRoles.noFields");
      fields.append(empty);
      return;
    }
    for (const control of shown) {
      const item = document.createElement("li");
      item.className = "form-role-field";
      const label = document.createElement("span");
      const kind = io.kindLabel(control.kind);
      label.textContent = control.title ? `${control.title} (${kind})` : `${kind}: ${control.text}`;
      item.append(label);
      if (control.paragraph) {
        const go = document.createElement("button");
        go.type = "button";
        go.className = "dialog-button";
        go.textContent = t("a11yCheck.goTo");
        go.setAttribute("aria-label", t("a11yCheck.goToNamed", { name: label.textContent }));
        go.addEventListener("click", () => {
          modal.close("goTo");
          io.select(control.paragraph, control.start);
        });
        item.append(go);
      }
      fields.append(item);
    }
  }

  function reflectFilter() {
    const names = rows()
      .map((row) => row.name.trim())
      .filter(Boolean);
    const current = filter.value || "*";
    fillSelect(filter, [["*", t("formRoles.allFields")], ["", t("formRoles.anyone")], ...names.map((name) => [name, name])], current);
    fillSelect(fillAs, [["", t("formRoles.anyone")], ...names.map((name) => [name, name])], fillAs.value);
    reflectFields();
  }

  function open() {
    const doc = io.getDoc();
    if (!doc) {
      io.setStatus(t("command.needsDocument"), "error");
      return;
    }
    const refusal = io.participantRefusal();
    if (refusal) {
      io.setStatus(refusal, "error");
      return;
    }
    if (!modal) return;
    opened = read();
    list.replaceChildren(...(opened.roles ?? []).map((role) => roleRow(role, role.name)));
    fillAs.value = opened.fillingAs ?? "";
    filter.value = "*";
    reflectFilter();
    fillAs.value = opened.fillingAs ?? "";
    modal.open();
  }

  async function apply() {
    const doc = io.getDoc();
    if (!doc || !opened) return;
    const sidecar = nextRoles(opened, rows(), fillAs.value);
    try {
      doc.setFormRoles(JSON.stringify(sidecar));
    } catch (error) {
      io.setStatus(String(error?.message ?? error), "error");
      return;
    }
    persist(sidecar);
    io.changed();
    modal.close("apply");
    io.setStatus(sidecar.fillingAs ? t("formRoles.fillingAs", { role: sidecar.fillingAs }) : t("formRoles.saved"));
  }

  add?.addEventListener("click", () => {
    const row = roleRow({ name: t("formRoles.newRole", { n: String(rows().length + 1) }), color: "3355C4" });
    list.append(row);
    reflectFilter();
    row.querySelector(".form-role-name")?.select();
    row.querySelector(".form-role-name")?.focus();
  });
  list?.addEventListener("input", (event) => {
    if (event.target?.classList?.contains("form-role-name")) reflectFilter();
  });
  filter?.addEventListener("change", reflectFields);
  save?.addEventListener("click", () => void apply());
  cancel?.addEventListener("click", () => modal?.close("cancel"));
  close?.addEventListener("click", () => modal?.close("cancel"));

  return { isProtected, open, protectForm, reflect, roles };
}
