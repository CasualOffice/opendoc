// SPDX-License-Identifier: Apache-2.0

//! Form roles: who fills which content control (`docs/153` `shell.form-roles`).
//!
//! # Where the roles live, and why not in the file
//!
//! ONLYOFFICE assigns each form field to a role ("Anyone", "Employee",
//! "Manager"…) and, while a person fills as one role, offers only that role's
//! fields (sdkjs `word/Editor/StructuredDocumentTags/FormsManager.js:246-259`,
//! `IsAllRequiredFormsFilled` skipping a field whose `GetFormRole()` is not the
//! current role). It stores the assignment in its own OFORM format — a field
//! master pointing at a user master (`SdtBase.js:262-294`) — not in anything
//! WordprocessingML defines.
//!
//! **OOXML has no place for a role.** ECMA-376's `w:sdtPr`, and the `w14`, `w15`
//! and `w16` extensions to it, carry an alias, a tag, a lock, a placeholder, a
//! data binding, a colour and an appearance, and nothing about who may fill the
//! control. Writing a role into `w:tag` would take the author's own tag from
//! them; inventing an `opendoc:` element would be markup Word drops on its next
//! save. So the roles are kept where ADR-030 I4 keeps derived and product data:
//! in a **sidecar beside the document**, never in the `v1` model.
//!
//! The sidecar is keyed by each control's `w:id` (`SdtProperties::control_id`)
//! rather than by its `NodeId`, because a `NodeId` is minted at open and the
//! `w:id` is what Word writes back and reads again: a role assigned here still
//! names the same control after the file goes through Word and returns. Every
//! control this editor inserts is given a unique `w:id`, and configuring one
//! that has none gives it one.
//!
//! The consequence, stated plainly: **the roles do not travel inside the
//! .docx.** The host persists them (`formRoles` / `setFormRoles`, the JSON the
//! editor's draft also keeps), the same way a host persists anything about a
//! document that is not the document.
//!
//! # What a role restricts
//!
//! Only forms protection gives a role anything to restrict: an unprotected
//! document is editable everywhere anyway. While the document is
//! forms-protected and a reader fills **as** a role, a control assigned to a
//! different role refuses input at the engine's choke point, exactly as a locked
//! control does. A control assigned to no role is anyone's, which is
//! ONLYOFFICE's default role.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::{WasmDocument, to_js};

/// The most roles a form may define. ONLYOFFICE's roles manager has no fixed
/// ceiling; a bound keeps a hostile sidecar from growing without one.
pub const MAX_ROLES: usize = 64;
/// The longest role name, in characters.
pub const MAX_ROLE_NAME: usize = 64;
/// The most assignments a sidecar may hold.
pub const MAX_ASSIGNMENTS: usize = 10_000;

/// One role.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormRole {
    /// The role's name, unique in the form.
    pub name: String,
    /// The colour its controls are marked with, six hex digits.
    pub color: String,
}

/// The sidecar: the roles, which control each is assigned, and who is filling.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormRoles {
    /// The roles, in the order the author listed them.
    #[serde(default)]
    pub roles: Vec<FormRole>,
    /// Control `w:id` → role name.
    #[serde(default)]
    pub assignments: BTreeMap<String, String>,
    /// The role the reader is filling as, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filling_as: Option<String>,
}

impl FormRoles {
    /// Validates a sidecar a host handed in, dropping assignments to roles that
    /// do not exist (a role deleted elsewhere leaves its controls anyone's).
    ///
    /// # Errors
    /// A role with no name, a name over [`MAX_ROLE_NAME`], a duplicate name, a
    /// colour that is not six hex digits, more than [`MAX_ROLES`] roles or
    /// [`MAX_ASSIGNMENTS`] assignments, or filling as a role that is not defined.
    pub fn validated(mut self) -> Result<Self, String> {
        if self.roles.len() > MAX_ROLES {
            return Err(format!("A form can have at most {MAX_ROLES} roles."));
        }
        if self.assignments.len() > MAX_ASSIGNMENTS {
            return Err(format!(
                "A form can assign at most {MAX_ASSIGNMENTS} controls to roles."
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for role in &mut self.roles {
            role.name = role.name.trim().to_owned();
            if role.name.is_empty() {
                return Err("A role needs a name.".to_owned());
            }
            if role.name.chars().count() > MAX_ROLE_NAME {
                return Err(format!(
                    "A role's name can be at most {MAX_ROLE_NAME} characters."
                ));
            }
            if !seen.insert(role.name.clone()) {
                return Err(format!(
                    "Two roles are called \u{201c}{}\u{201d}.",
                    role.name
                ));
            }
            let color = role.color.trim_start_matches('#');
            if color.len() != 6 || !color.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("\u{201c}{}\u{201d} is not a colour.", role.color));
            }
            role.color = color.to_ascii_uppercase();
        }
        self.assignments.retain(|_, role| seen.contains(role));
        if let Some(role) = &self.filling_as
            && !seen.contains(role)
        {
            return Err(format!("There is no role called \u{201c}{role}\u{201d}."));
        }
        Ok(self)
    }

    /// The role a control is assigned, by its `w:id`. O(log assignments).
    #[must_use]
    pub fn role_of(&self, control_id: Option<&str>) -> Option<&str> {
        control_id
            .and_then(|id| self.assignments.get(id))
            .map(String::as_str)
    }

    /// Whether the reader's role lets them fill a control: always without a
    /// role, and for a control assigned to no role or to theirs.
    #[must_use]
    pub fn may_fill(&self, control_id: Option<&str>) -> bool {
        match (&self.filling_as, self.role_of(control_id)) {
            (Some(me), Some(theirs)) => me == theirs,
            _ => true,
        }
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// The form-roles sidecar as JSON: `{roles: [{name, color}], assignments:
    /// {"<w:id>": "<role>"}, fillingAs?}`. Not part of the document (this
    /// module's comment says why): the host persists it. O(roles + assignments).
    #[wasm_bindgen(js_name = formRoles)]
    #[must_use]
    pub fn form_roles_json(&self) -> String {
        serde_json::to_string(&self.form_roles).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Replaces the form-roles sidecar.
    ///
    /// # Errors
    /// Malformed JSON, or what [`FormRoles::validated`] refuses.
    #[wasm_bindgen(js_name = setFormRoles)]
    pub fn set_form_roles(&mut self, json: &str) -> Result<(), JsValue> {
        self.set_form_roles_inner(json).map_err(to_js)
    }

    /// Fills as `role` from now on, or as nobody in particular with `None`.
    ///
    /// # Errors
    /// When `role` is not defined.
    #[wasm_bindgen(js_name = setFormFillRole)]
    pub fn set_form_fill_role(&mut self, role: Option<String>) -> Result<(), JsValue> {
        self.set_form_fill_role_inner(role).map_err(to_js)
    }
}

impl WasmDocument {
    pub(crate) fn set_form_roles_inner(&mut self, json: &str) -> Result<(), String> {
        let roles: FormRoles = serde_json::from_str(json)
            .map_err(|error| format!("The form roles could not be read: {error}"))?;
        self.form_roles = roles.validated()?;
        Ok(())
    }

    pub(crate) fn set_form_fill_role_inner(&mut self, role: Option<String>) -> Result<(), String> {
        let mut next = self.form_roles.clone();
        next.filling_as = role.filter(|name| !name.is_empty());
        self.form_roles = next.validated()?;
        Ok(())
    }
}
