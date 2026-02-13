use std::fs;
use std::path::Path;

use log::info;

use crate::style::{color_map_for_theme, THEME_NAMES};

/// Generate CSS string for a given theme, matching njord's create_sheet.py output.
fn generate_css(theme_name: &str) -> String {
    let colors = color_map_for_theme(theme_name);

    let nodta = colors.get("NODTA").and_then(|v| v.as_str()).unwrap_or("#000000");
    let cursr = colors.get("CURSR").and_then(|v| v.as_str()).unwrap_or("#000000");

    let mut css = format!(
        "svg {{\n    background-color: {nodta};\n    color: {cursr};\n}}\n\
         .layout {{display:none}}\n\
         .symbolBox {{stroke:black;stroke-width:0.32;}}\n\
         .svgBox {{stroke:blue;stroke-width:0.32;}}\n\
         .pivotPoint {{stroke:red;stroke-width:0.64;}}\n\
         .sl {{stroke-linecap:round;stroke-linejoin:round}}\n\
         .f0 {{fill:none}}\n"
    );

    let mut tokens: Vec<&String> = colors.keys().collect();
    tokens.sort();

    for token in tokens {
        if let Some(hex) = colors.get(token.as_str()).and_then(|v| v.as_str()) {
            css.push_str(&format!(".s{token} {{stroke:{hex}}}\n"));
            css.push_str(&format!(".f{token} {{fill:{hex}}}\n"));
        }
    }

    css
}

/// Generate themed sprite directories with CSS inlined into each SVG.
pub fn generate_themed_sprites(svg_source_dir: &Path, output_dir: &Path) {
    for &theme in THEME_NAMES {
        let css = generate_css(theme);
        let theme_dir = output_dir.join(theme);
        fs::create_dir_all(&theme_dir).expect("Failed to create theme output directory");

        let mut count = 0;
        for entry in fs::read_dir(svg_source_dir).expect("Failed to read SVG source directory") {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "svg") {
                let svg_content = fs::read_to_string(&path).expect("Failed to read SVG file");
                let themed_svg =
                    svg_content.replace("</svg>", &format!("<defs><style>{css}</style></defs></svg>"));

                let dest = theme_dir.join(entry.file_name());
                fs::write(&dest, themed_svg).expect("Failed to write themed SVG");
                count += 1;
            }
        }

        info!("Generated {} themed SVGs for '{}'", count, theme);
    }
}
