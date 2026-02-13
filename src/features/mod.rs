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
        _ => None,
    }
    .map(String::from);
    StyleProps {
        ac,
        lc: Some("CHGRD".into()),
        sy: None,
    }
}

fn lndare_style(_attrs: &Map<String, Value>) -> StyleProps {
    StyleProps {
        ac: Some("LANDA".into()),
        lc: Some("CSTLN".into()),
        sy: Some("LNDARE01".into()),
    }
}

fn lights_style(attrs: &Map<String, Value>) -> StyleProps {
    // Select symbol based on category of light (CATLIT) and colour (COLOUR)
    // CATLIT values: 1=directional, 4=leading, 8=aero, etc.
    // COLOUR values: 1=white, 3=red, 4=green, 6=yellow
    let catlit = attrs.get("CATLIT").and_then(|v| v.as_i64());
    let colour = attrs.get("COLOUR").and_then(|v| {
        v.as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_i64())
    });

    let symbol = match (catlit, colour) {
        // Aero lights (CATLIT=8) - use LIGHTS81/82
        (Some(8), Some(3)) => "LIGHTS81", // red aero light
        (Some(8), _) => "LIGHTS82",       // other aero lights
        // Standard lights by colour
        (_, Some(3)) => "LIGHTS11", // red light
        (_, Some(4)) => "LIGHTS12", // green light
        (_, Some(6)) => "LIGHTS13", // yellow light
        // Default to white/general light
        _ => "LIGHTS11",
    };

    StyleProps {
        ac: None,
        lc: None,
        sy: Some(symbol.into()),
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
        StyleLayerDef {
            id_suffix: "fill",
            layer_type: StyleLayerType::Fill,
            colors: &["DEPIT", "DEPVS", "DEPMS", "DEPMD", "DEPDW"],
            line_width: None,
        },
        StyleLayerDef {
            id_suffix: "line",
            layer_type: StyleLayerType::Line,
            colors: &["CHGRD"],
            line_width: Some(0.5),
        },
    ],
};

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
        StyleLayerDef {
            id_suffix: "fill",
            layer_type: StyleLayerType::Fill,
            colors: &["LANDA"],
            line_width: None,
        },
        StyleLayerDef {
            id_suffix: "line",
            layer_type: StyleLayerType::Line,
            colors: &["CSTLN"],
            line_width: Some(2.0),
        },
        StyleLayerDef {
            id_suffix: "symbol",
            layer_type: StyleLayerType::Symbol,
            colors: &[],
            line_width: None,
        },
    ],
};

pub const LIGHTS: LayerDef = LayerDef {
    s57_name: "LIGHTS",
    table: "lights",
    columns: &[
        ColumnDef::new("CATLIT", "catlit", ColType::Int),
        ColumnDef::new("COLOUR", "colour", ColType::Int),
        ColumnDef::new("LITCHR", "litchr", ColType::Int),
        ColumnDef::new("SIGPER", "sigper", ColType::Float),
        ColumnDef::new("VALNMR", "valnmr", ColType::Float),
        ColumnDef::new("HEIGHT", "height", ColType::Float),
        ColumnDef::new("OBJNAM", "objnam", ColType::Text),
    ],
    style_fn: Some(lights_style),
    style_layers: &[StyleLayerDef {
        id_suffix: "symbol",
        layer_type: StyleLayerType::Symbol,
        colors: &[],
        line_width: None,
    }],
};

pub fn all_layers() -> &'static [&'static LayerDef] {
    &[&DEPARE, &LNDARE, &LIGHTS]
}
