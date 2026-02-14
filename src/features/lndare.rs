use serde_json::{Map, Value};

use crate::feature::{ColType, ColumnDef, LayerDef, StyleProps};
use crate::style::{StyleLayerDef, StyleLayerType};

fn lndare_style(_attrs: &Map<String, Value>) -> StyleProps {
    StyleProps {
        ac: Some("LANDA".into()),
        lc: Some("CSTLN".into()),
        sy: Some("LNDARE01".into()),
    }
}

pub const LNDARE: LayerDef = LayerDef {
    s57_name: "LNDARE",
    table: "lndare",
    columns: &[
        ColumnDef::new("OBJNAM", "objnam", ColType::Text),
        ColumnDef::new("CONDTN", "condtn", ColType::Int),
        ColumnDef::new("NATSUR", "natsur", ColType::Int),
        ColumnDef::new("NATQUA", "natqua", ColType::Int),
    ],
    style_fn: Some(lndare_style),
    style_layers: &[
        StyleLayerDef::new("fill", StyleLayerType::Fill).with_colors(&["LANDA"]),
        StyleLayerDef::new("line", StyleLayerType::Line)
            .with_colors(&["CSTLN"])
            .with_line_width(2.0),
        StyleLayerDef::new("icon", StyleLayerType::Icon),
    ],
};
