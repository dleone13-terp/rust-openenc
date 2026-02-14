use serde_json::{Map, Value};

use crate::feature::{ColType, ColumnDef, LayerDef, StyleProps};
use crate::style::{StyleLayerDef, StyleLayerType};

fn depare_style(attrs: &Map<String, Value>) -> StyleProps {
    let drval1 = attrs.get("DRVAL1").and_then(|v| v.as_f64());
    let drval2 = attrs.get("DRVAL2").and_then(|v| v.as_f64());
    let ac = match (drval1, drval2) {
        (Some(d1), Some(d2)) if d1 < 0.0 && d2 <= 0.0 => Some("DEPIT"),
        (Some(d1), _) if d1 <= 3.0 => Some("DEPVS"),
        (Some(d1), _) if d1 <= 6.0 => Some("DEPMS"),
        (Some(d1), _) if d1 <= 9.0 => Some("DEPMD"),
        (Some(d1), _) if d1 > 9.0 => Some("DEPDW"),
        _ => Some("DEPDW"), // Default to deep water when depth range is unknown
    }
    .map(String::from);
    StyleProps {
        ac,
        lc: Some("CHGRD".into()),
        sy: None,
    }
}

pub const DEPARE: LayerDef = LayerDef {
    s57_name: "DEPARE",
    table: "depare",
    columns: &[
        ColumnDef::new("DRVAL1", "drval1", ColType::Float),
        ColumnDef::new("DRVAL2", "drval2", ColType::Float),
    ],
    style_fn: Some(depare_style),
    style_layers: &[
        StyleLayerDef::new("fill", StyleLayerType::Fill)
            .with_colors(&["DEPIT", "DEPVS", "DEPMS", "DEPMD", "DEPDW"]),
        StyleLayerDef::new("line", StyleLayerType::Line)
            .with_colors(&["CHGRD"])
            .with_line_width(0.5),
    ],
};
