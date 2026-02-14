use serde_json::{Map, Value};

use crate::feature::{ColType, ColumnDef, LayerDef, StyleProps};
use crate::style::{StyleLayerDef, StyleLayerType};

fn soundg_style(attrs: &Map<String, Value>) -> StyleProps {
    // Soundings use depth-conditional colors:
    // SNDG2 (black) for shallow depths (<9m), SNDG1 (gray) for deep depths (≥9m)
    let depth = attrs.get("DEPTH").and_then(|v| v.as_f64());
    let ac = match depth {
        Some(d) if d < 9.0 => Some("SNDG2".into()), // Shallow: black #000000
        Some(_) => Some("SNDG1".into()),            // Deep: gray #768C97
        None => None,
    };

    StyleProps {
        ac,
        lc: None,
        sy: None,
    }
}

pub const SOUNDG: LayerDef = LayerDef {
    s57_name: "SOUNDG",
    table: "soundg",
    columns: &[
        // DEPTH extracted from geometry Z-coordinate (meters)
        ColumnDef::new("DEPTH", "depth", ColType::Float),
        ColumnDef::new("TECSOU", "tecsou", ColType::Int),
        ColumnDef::new("QUASOU", "quasou", ColType::Int),
        ColumnDef::new("STATUS", "status", ColType::Int),
    ],
    style_fn: Some(soundg_style),
    style_layers: &[StyleLayerDef::new("text", StyleLayerType::Text)
        .with_text("depth_meters_whole", 16.0)
        .with_colors(&["SNDG1", "SNDG2"])
        .with_text_anchor("top")
        .with_text_offset(0.0, 0.5)
        .with_text_halo(2.5)
        .with_text_halo_color("#FFFFFF")
        .use_area_color_for_text()],
};
