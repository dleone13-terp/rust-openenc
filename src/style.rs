use serde_json::{json, Map, Value};
use std::sync::LazyLock;

use crate::feature::LayerDef;

/// Mapbox GL layer type
#[derive(Clone, Copy)]
pub enum StyleLayerType {
    Fill,
    Line,
    Symbol,
}

/// Declarative description of one Mapbox GL style layer for a feature type.
pub struct StyleLayerDef {
    pub id_suffix: &'static str,
    pub layer_type: StyleLayerType,
    pub colors: &'static [&'static str],
    pub line_width: Option<f64>,
    /// Property name to use for text-field (e.g., "depth")
    pub text_field: Option<&'static str>,
    /// Text size in pixels
    pub text_size: Option<f64>,
}

impl StyleLayerDef {
    /// Create a new StyleLayerDef with defaults (empty colors, no line width, no text)
    pub const fn new(id_suffix: &'static str, layer_type: StyleLayerType) -> Self {
        Self {
            id_suffix,
            layer_type,
            colors: &[],
            line_width: None,
            text_field: None,
            text_size: None,
        }
    }

    /// Set the colors for area/line styling
    pub const fn with_colors(mut self, colors: &'static [&'static str]) -> Self {
        self.colors = colors;
        self
    }

    /// Set the line width
    pub const fn with_line_width(mut self, width: f64) -> Self {
        self.line_width = Some(width);
        self
    }

    /// Set text field and optional size
    pub const fn with_text(mut self, field: &'static str, size: f64) -> Self {
        self.text_field = Some(field);
        self.text_size = Some(size);
        self
    }
}

pub const THEME_NAMES: &[&str] = &["day", "dusk", "night"];

static COLORS_JSON: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../colors.json")).unwrap()
});

pub fn color_map_for_theme(theme_name: &str) -> &Map<String, Value> {
    COLORS_JSON["library"][theme_name.to_uppercase()]
        .as_object()
        .unwrap_or_else(|| panic!("Unknown theme '{}'", theme_name))
}

fn build_case_expression(prop: &str, tokens: &[&str], colors: &Map<String, Value>) -> Value {
    let mut expr: Vec<Value> = vec![json!("case")];
    for &token in tokens {
        if let Some(hex) = colors.get(token).and_then(|v| v.as_str()) {
            expr.push(json!(["==", ["get", prop], token]));
            expr.push(json!(hex));
        }
    }
    expr.push(json!("rgba(0,0,0,0)"));
    Value::Array(expr)
}

pub fn generate_style_json(layers: &[&LayerDef], theme_name: &str, tile_source_url: &str) -> String {
    let colors = color_map_for_theme(theme_name);

    let mut style_layers: Vec<Value> = Vec::new();

    for layer_def in layers {
        for sld in layer_def.style_layers {
            let id = format!("{}_{}", layer_def.table, sld.id_suffix);
            let mut layer = json!({
                "id": id,
                "source": layer_def.table,
                "source-layer": layer_def.table,
            });

            match sld.layer_type {
                StyleLayerType::Fill => {
                    layer["type"] = json!("fill");
                    layer["paint"] = json!({
                        "fill-color": build_case_expression("AC", sld.colors, colors),
                    });
                }
                StyleLayerType::Line => {
                    layer["type"] = json!("line");
                    let mut paint = json!({
                        "line-color": build_case_expression("LC", sld.colors, colors),
                    });
                    if let Some(w) = sld.line_width {
                        paint["line-width"] = json!(w);
                    }
                    layer["paint"] = paint;
                }
                StyleLayerType::Symbol => {
                    layer["type"] = json!("symbol");
                    let mut layout = json!({
                        "icon-image": ["get", "SY"],
                    });
                    
                    // Add text-field if specified
                    if let Some(text_field) = sld.text_field {
                        layout["text-field"] = json!(["to-string", ["round", ["get", text_field]]]);
                        layout["text-font"] = json!(["Open Sans Regular"]);
                        layout["text-size"] = json!(sld.text_size.unwrap_or(12.0));
                        layout["text-anchor"] = json!("top");
                        layout["text-offset"] = json!([0.0, 0.5]);
                        
                        // Add text paint properties
                        layer["paint"] = json!({
                            "text-color": "#000000",
                            "text-halo-color": "#FFFFFF",
                            "text-halo-width": 1.5,
                        });
                    }
                    
                    layer["layout"] = layout;
                }
            }

            style_layers.push(layer);
        }
    }

    let mut sources = serde_json::Map::new();
    for layer_def in layers {
        sources.insert(
            layer_def.table.to_string(),
            json!({
                "type": "vector",
                "tiles": [format!("{}/{}_mvt/{{z}}/{{x}}/{{y}}", tile_source_url, layer_def.table)],
            }),
        );
    }

    let style = json!({
        "version": 8,
        "name": format!("openenc-{}", theme_name),
        "sprite": format!("{}/sprite/{}", tile_source_url, theme_name),
        "glyphs": format!("{}/font/{{fontstack}}/{{range}}", tile_source_url),
        "sources": sources,
        "layers": style_layers,
    });

    serde_json::to_string_pretty(&style).expect("Failed to serialize style JSON")
}
