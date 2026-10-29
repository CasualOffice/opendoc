//! Additive character formatting for the existing atomic FormatText operation.
use casual_doc_model::v1::{BorderEdge, EmphasisMark, Ligatures, RunProperties};
use serde::{Deserialize, Deserializer, Serialize};

// Missing leaves a property alone; explicit null clears direct formatting.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// A property-preserving character-formatting patch. All numbers use model units.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterFormatDelta {
    /// Set or clear `all_caps`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub all_caps: Option<Option<bool>>,
    /// Set or clear `small_caps`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub small_caps: Option<Option<bool>>,
    /// Set or clear `double_strike`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub double_strike: Option<Option<bool>>,
    /// Set or clear `outline`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub outline: Option<Option<bool>>,
    /// Set or clear `shadow`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub shadow: Option<Option<bool>>,
    /// Set or clear `emboss`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub emboss: Option<Option<bool>>,
    /// Set or clear `imprint`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub imprint: Option<Option<bool>>,
    /// Set or clear `emphasis`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub emphasis: Option<Option<EmphasisMark>>,
    /// Set or clear `border`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub border: Option<Option<BorderEdge>>,
    /// Set or clear `character_spacing_twips`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub character_spacing_twips: Option<Option<i32>>,
    /// Set or clear `character_scale_percent`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub character_scale_percent: Option<Option<u16>>,
    /// Set or clear `position_half_points`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_half_points: Option<Option<i32>>,
    /// Set or clear `kerning_half_points`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub kerning_half_points: Option<Option<u32>>,
    /// Set or clear `ligatures`; absent preserves the existing value.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub ligatures: Option<Option<Ligatures>>,
    /// Set or clear the Latin language tag (`w:lang@w:val`, a BCP 47 tag such as
    /// `fr-FR`); absent preserves it. The East Asian and complex-script tags
    /// are left as they are, which is what Word's Set Proofing Language does to
    /// a Latin selection (`docs/153` `text.language-of-selection`). Clearing
    /// the Latin tag of a run that carries no other tag removes `w:lang`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub language: Option<Option<String>>,
}
impl CharacterFormatDelta {
    /// Remove exactly the properties a later concurrent patch writes.
    pub fn without(&self, other: &Self) -> Option<Box<Self>> {
        let Self {
            all_caps,
            small_caps,
            double_strike,
            outline,
            shadow,
            emboss,
            imprint,
            emphasis,
            border,
            character_spacing_twips,
            character_scale_percent,
            position_half_points,
            kerning_half_points,
            ligatures,
            language,
        } = self.clone();
        let remaining = Self {
            all_caps: all_caps.filter(|_| other.all_caps.is_none()),
            small_caps: small_caps.filter(|_| other.small_caps.is_none()),
            double_strike: double_strike.filter(|_| other.double_strike.is_none()),
            outline: outline.filter(|_| other.outline.is_none()),
            shadow: shadow.filter(|_| other.shadow.is_none()),
            emboss: emboss.filter(|_| other.emboss.is_none()),
            imprint: imprint.filter(|_| other.imprint.is_none()),
            emphasis: emphasis.filter(|_| other.emphasis.is_none()),
            border: border.filter(|_| other.border.is_none()),
            character_spacing_twips: character_spacing_twips
                .filter(|_| other.character_spacing_twips.is_none()),
            character_scale_percent: character_scale_percent
                .filter(|_| other.character_scale_percent.is_none()),
            position_half_points: position_half_points
                .filter(|_| other.position_half_points.is_none()),
            kerning_half_points: kerning_half_points
                .filter(|_| other.kerning_half_points.is_none()),
            ligatures: ligatures.filter(|_| other.ligatures.is_none()),
            language: language.filter(|_| other.language.is_none()),
        };
        (remaining != Self::default()).then(|| Box::new(remaining))
    }
    /// Apply only supplied fields; document validation remains authoritative.
    pub fn apply_to(&self, props: &mut RunProperties) {
        if let Some(value) = &self.all_caps {
            props.all_caps = *value;
        }
        if let Some(value) = &self.small_caps {
            props.small_caps = *value;
        }
        if let Some(value) = &self.double_strike {
            props.double_strike = *value;
        }
        if let Some(value) = &self.outline {
            props.outline = *value;
        }
        if let Some(value) = &self.shadow {
            props.shadow = *value;
        }
        if let Some(value) = &self.emboss {
            props.emboss = *value;
        }
        if let Some(value) = &self.imprint {
            props.imprint = *value;
        }
        if let Some(value) = &self.emphasis {
            props.emphasis = *value;
        }
        if let Some(value) = &self.border {
            props.border = value.clone();
        }
        if let Some(value) = &self.character_spacing_twips {
            props.character_spacing_twips = *value;
        }
        if let Some(value) = &self.character_scale_percent {
            props.character_scale_percent = *value;
        }
        if let Some(value) = &self.position_half_points {
            props.position_half_points = *value;
        }
        if let Some(value) = &self.kerning_half_points {
            props.kerning_half_points = *value;
        }
        if let Some(value) = &self.ligatures {
            props.ligatures = *value;
        }
        if let Some(value) = &self.language {
            let mut language = props.language.take().unwrap_or_default();
            language.value.clone_from(value);
            props.language = (!language.is_empty()).then_some(language);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::Language;
    #[test]
    fn absent_null_and_false_are_distinct_and_other_properties_survive() {
        let mut props = RunProperties {
            bold: Some(true),
            all_caps: Some(true),
            shadow: Some(true),
            ..Default::default()
        };
        let patch: CharacterFormatDelta =
            serde_json::from_str(r#"{"allCaps":null,"shadow":false,"characterSpacingTwips":30}"#)
                .unwrap();
        patch.apply_to(&mut props);
        assert_eq!(props.all_caps, None);
        assert_eq!(props.shadow, Some(false));
        assert_eq!(props.bold, Some(true));
        assert_eq!(props.character_spacing_twips, Some(30));
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            serde_json::json!({"allCaps":null,"shadow":false,"characterSpacingTwips":30})
        );
        assert!(serde_json::from_str::<CharacterFormatDelta>(r#"{"unknown":true}"#).is_err());
    }

    #[test]
    fn language_sets_and_clears_only_the_latin_tag() {
        let mut props = RunProperties {
            language: Some(Language {
                value: Some("en-US".into()),
                east_asia: Some("ja-JP".into()),
                bidi: None,
            }),
            ..Default::default()
        };
        let set: CharacterFormatDelta = serde_json::from_str(r#"{"language":"fr-FR"}"#).unwrap();
        set.apply_to(&mut props);
        let language = props.language.as_ref().unwrap();
        assert_eq!(language.value.as_deref(), Some("fr-FR"));
        assert_eq!(language.east_asia.as_deref(), Some("ja-JP"));
        let clear: CharacterFormatDelta = serde_json::from_str(r#"{"language":null}"#).unwrap();
        clear.apply_to(&mut props);
        assert_eq!(props.language.as_ref().unwrap().value, None);
        props.language.as_mut().unwrap().east_asia = None;
        clear.apply_to(&mut props);
        assert_eq!(
            props.language, None,
            "an empty w:lang is removed, not kept empty"
        );
        // A concurrent patch that writes the language wins over this one's.
        assert_eq!(set.without(&set), None);
    }
}
