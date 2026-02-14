use serde_json::{Map, Value};

use crate::colors::{parse_colours, Colour};
use crate::feature::{ColType, ColumnDef, LayerDef, StyleProps};
use crate::style::{StyleLayerDef, StyleLayerType};

fn lights_style(attrs: &Map<String, Value>) -> StyleProps {
    // Parse all colors from COLOUR attribute (can be array for multi-color lights)
    // Currently uses first color for symbol selection, but parsing all colors
    // enables future multi-color rendering support
    let colours = parse_colours(attrs);
    
    // Select symbol based on category of light (CATLIT) and colour
    // CATLIT values: 1=directional, 4=leading, 8=aero, etc.
    let catlit = attrs.get("CATLIT").and_then(|v| v.as_i64());

    let symbol = match (catlit, colours.first()) {
        // Aero lights (CATLIT=8) - use LIGHTS81/82
        (Some(8), Some(Colour::Red)) => "LIGHTS81", // red aero light
        (Some(8), _) => "LIGHTS82",                  // other aero lights
        // Standard lights by colour (using first color for symbol selection)
        (_, Some(Colour::Red)) => "LIGHTS11",     // red light
        (_, Some(Colour::Green)) => "LIGHTS12",   // green light
        (_, Some(Colour::Yellow)) => "LIGHTS13",  // yellow light
        (_, Some(Colour::White | Colour::Amber | Colour::Orange | Colour::Magenta)) => "LITDEF11",
        // Default to general light symbol
        _ => "LITDEF11",
    };

    StyleProps {
        ac: None,
        lc: None,
        sy: Some(symbol.into()),
    }
}

pub const LIGHTS: LayerDef = LayerDef {
    s57_name: "LIGHTS",
    table: "lights",
    columns: &[
        ColumnDef::new("CATLIT", "catlit", ColType::Int),
        ColumnDef::new("COLOUR", "colour", ColType::Int), // Stores first color only
        ColumnDef::new("LITCHR", "litchr", ColType::Int),
        ColumnDef::new("SIGPER", "sigper", ColType::Float),
        ColumnDef::new("VALNMR", "valnmr", ColType::Float),
        ColumnDef::new("HEIGHT", "height", ColType::Float),
        ColumnDef::new("OBJNAM", "objnam", ColType::Text),
    ],
    style_fn: Some(lights_style),
    style_layers: &[StyleLayerDef::new("icon", StyleLayerType::Icon)],
};
