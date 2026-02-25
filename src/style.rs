use serde_json::{Map, Value, json};
use std::sync::LazyLock;

use crate::feature::LayerDef;

/// Mapbox GL layer type
#[derive(Clone, Copy)]
pub enum StyleLayerType {
    Fill,
    Line,
    Icon,
    Text,
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
    /// Text halo width in pixels
    pub text_halo_width: Option<f64>,
    /// Text halo color (hex string, e.g., "#FFFFFF")
    pub text_halo_color: Option<&'static str>,
    /// Text anchor position (e.g., "top", "center", "bottom-right")
    pub text_anchor: Option<&'static str>,
    /// Text offset [x, y] in ems
    pub text_offset: Option<(f64, f64)>,
    /// Use AC (area color) token for text color instead of fixed black
    pub area_color_for_text: bool,
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
            text_halo_width: None,
            text_halo_color: None,
            text_anchor: None,
            text_offset: None,
            area_color_for_text: false,
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

    /// Set text halo width
    pub const fn with_text_halo(mut self, width: f64) -> Self {
        self.text_halo_width = Some(width);
        self
    }

    /// Set text halo color
    pub const fn with_text_halo_color(mut self, color: &'static str) -> Self {
        self.text_halo_color = Some(color);
        self
    }

    /// Set text anchor position
    pub const fn with_text_anchor(mut self, anchor: &'static str) -> Self {
        self.text_anchor = Some(anchor);
        self
    }

    /// Set text offset [x, y] in ems
    pub const fn with_text_offset(mut self, x: f64, y: f64) -> Self {
        self.text_offset = Some((x, y));
        self
    }

    /// Use AC (area color) token for text color
    pub const fn use_area_color_for_text(mut self) -> Self {
        self.area_color_for_text = true;
        self
    }
}

pub const THEME_NAMES: &[&str] = &["day", "dusk", "night"];

static COLORS_JSON: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("../colors.json")).unwrap());

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

pub fn generate_style_json(
    layers: &[&LayerDef],
    theme_name: &str,
    tile_source_url: &str,
) -> String {
    let colors = color_map_for_theme(theme_name);

    let mut style_layers: Vec<Value> = Vec::new();

    for layer_def in layers {
        for sld in layer_def.style_layers {
            let id = format!("{}_{}", layer_def.table, sld.id_suffix);
            let mut layer = json!({
                "id": id,
                "source": "enc",
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
                StyleLayerType::Icon => {
                    layer["type"] = json!("symbol");
                    layer["layout"] = json!({
                        "icon-image": ["get", "SY"],
                    });
                }
                StyleLayerType::Text => {
                    layer["type"] = json!("symbol");
                    let mut layout = json!({});

                    if let Some(text_field) = sld.text_field {
                        layout["text-field"] = json!(["to-string", ["get", text_field]]);

                        // Standardized font for all text layers
                        layout["text-font"] = json!(["Roboto Bold"]);

                        // Other text styling comes from StyleLayerDef (configured in feature files)
                        if let Some(size) = sld.text_size {
                            layout["text-size"] = json!(size);
                        }
                        if let Some(anchor) = sld.text_anchor {
                            layout["text-anchor"] = json!(anchor);
                        }
                        if let Some((x, y)) = sld.text_offset {
                            layout["text-offset"] = json!([x, y]);
                        }

                        // Add text paint properties
                        let text_color = if sld.area_color_for_text && !sld.colors.is_empty() {
                            build_case_expression("AC", sld.colors, colors)
                        } else {
                            json!("#000000")
                        };

                        let mut paint = json!({
                            "text-color": text_color,
                        });

                        if let Some(halo_color) = sld.text_halo_color {
                            paint["text-halo-color"] = json!(halo_color);
                        }
                        if let Some(halo_width) = sld.text_halo_width {
                            paint["text-halo-width"] = json!(halo_width);
                        }

                        layer["paint"] = paint;
                    }

                    layer["layout"] = layout;
                }
            }

            style_layers.push(layer);
        }
    }

    let mut sources = serde_json::Map::new();
    sources.insert(
        "enc".to_string(),
        json!({
            "type": "vector",
            "tiles": [format!("{}/enc_mvt/{{z}}/{{x}}/{{y}}", tile_source_url)],
            "minzoom": 0,
            "maxzoom": 14,
        }),
    );

    let style = json!({
        "version": 8,
        "name": format!("openenc-{}", theme_name),
        "sprite": format!("{}/sprite/{}", tile_source_url, theme_name),
        "glyphs": format!("{}/font/{{fontstack}}/{{range}}", tile_source_url),
        "center": [-122.3321, 47.6062],
        "zoom": 8,
        "sources": sources,
        "layers": style_layers,
    });

    serde_json::to_string_pretty(&style).expect("Failed to serialize style JSON")
}
