//! Layers and sheets.
//!
//! A layer carries a drawing colour, a visibility flag and a lock flag.
//! Hidden or locked layers are left out of hit tests, probing and selection,
//! which is how the 2D view stops the user from picking work they have put
//! aside. A sheet is one piece of material: every entity belongs to exactly
//! one sheet and the views show one sheet at a time.

use serde::{Deserialize, Serialize};

/// Colours new layers cycle through, in order. The first layer of a document
/// is black, the usual first layer.
pub const LAYER_COLOURS: [&str; 7] = [
    "#000000", "#d62828", "#1f6fd6", "#2a9d3f", "#b5179e", "#e07c00", "#008b8b",
];

/// One drawing layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    /// Document wide id, never reused.
    pub id: u64,
    /// Name shown in the layers panel.
    pub name: String,
    /// Drawing colour as `#rrggbb`.
    pub colour: String,
    /// Whether the layer's entities are drawn.
    pub visible: bool,
    /// Whether the layer's entities can be picked or edited.
    pub locked: bool,
    /// Whether the layer's closed vectors mark keep-out zones (clamps and
    /// fixtures) that toolpaths are checked against.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_out: bool,
}

impl Layer {
    /// A visible, unlocked layer with the given id, name and colour.
    pub fn new(id: u64, name: impl Into<String>, colour: impl Into<String>) -> Self {
        Layer {
            id,
            name: name.into(),
            colour: colour.into(),
            visible: true,
            locked: false,
            keep_out: false,
        }
    }

    /// The colour a layer gets when the client does not pick one. Layers
    /// cycle through [`LAYER_COLOURS`] by their position in the list.
    pub fn colour_for(index: usize) -> &'static str {
        LAYER_COLOURS[index % LAYER_COLOURS.len()]
    }
}

/// One sheet of material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sheet {
    /// Document wide id, never reused.
    pub id: u64,
    /// Name shown in the sheets panel.
    pub name: String,
    /// Size, thickness and datum of the sheet while another sheet is
    /// active; the job setup holds the active sheet's. `None` (older files)
    /// takes the job setup's when the sheet becomes active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<super::sheets::SheetMaterial>,
}

impl Sheet {
    /// A sheet with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Sheet {
            id,
            name: name.into(),
            material: None,
        }
    }
}

/// True when the text is a `#rrggbb` colour.
pub fn is_hex_colour(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_colours_cycle_through_the_palette() {
        assert_eq!(Layer::colour_for(0), "#000000");
        assert_eq!(Layer::colour_for(7), "#000000");
        assert_eq!(Layer::colour_for(1), "#d62828");
    }

    #[test]
    fn new_layers_are_visible_and_unlocked() {
        let layer = Layer::new(4, "Cut", "#000000");
        assert!(layer.visible);
        assert!(!layer.locked);
        assert_eq!(layer.id, 4);
    }

    #[test]
    fn colours_are_checked_for_shape() {
        assert!(is_hex_colour("#00ff7f"));
        assert!(!is_hex_colour("00ff7f"));
        assert!(!is_hex_colour("#00ff7"));
        assert!(!is_hex_colour("#00ff7g"));
    }

    #[test]
    fn a_layer_round_trips_through_json_in_camel_case() {
        let layer = Layer::new(1, "Layer 1", "#000000");
        let text = serde_json::to_string(&layer).expect("serialises");
        assert!(text.contains("\"colour\":\"#000000\""), "{text}");
        let back: Layer = serde_json::from_str(&text).expect("parses");
        assert_eq!(back, layer);
    }

    #[test]
    fn sheets_carry_a_name() {
        let sheet = Sheet::new(1, "Sheet 1");
        assert_eq!(sheet.name, "Sheet 1");
    }
}
