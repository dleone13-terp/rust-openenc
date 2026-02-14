use gdal::Dataset;
use gdal::vector::LayerAccess;
use log::{debug, error, warn};
use std::fs;
use std::path::{Path, PathBuf};

/// S-57 metadata extracted from DSID layer
#[derive(Debug, Clone)]
pub struct S57Metadata {
    pub edition: Option<i32>,
    pub update_number: i32,
    pub compilation_scale: i32,
}

/// Extract S-57 metadata from DSID layer, including DSPM_CSCL compilation scale
pub fn extract_metadata(dataset: &Dataset) -> S57Metadata {
    for layer_idx in 0..dataset.layer_count() {
        if let Ok(mut layer) = dataset.layer(layer_idx) {
            if layer.name().eq_ignore_ascii_case("DSID") {
                if let Some(feature) = layer.features().next() {
                    let mut edition = None;
                    let mut update_number = 0;
                    let mut compilation_scale = 0;

                    for (field_name, field_value_opt) in feature.fields() {
                        if let Some(field_value) = field_value_opt {
                            match field_name.to_uppercase().as_str() {
                                "EDTN" => edition = field_value.into_int(),
                                "UPDN" => update_number = field_value.into_int().unwrap_or(0),
                                "DSPM_CSCL" => {
                                    compilation_scale = field_value.into_int().unwrap_or(0)
                                }
                                _ => {}
                            }
                        }
                    }

                    return S57Metadata {
                        edition,
                        update_number,
                        compilation_scale,
                    };
                }
            }
        }
    }

    S57Metadata {
        edition: None,
        update_number: 0,
        compilation_scale: 0,
    }
}

/// Extract coverage polygon from M_COVR layer (CATCOV=1 features).
/// Returns a GeoJSON geometry string, or None if M_COVR is not found.
pub fn extract_coverage_geojson(dataset: &Dataset) -> Option<String> {
    for layer_idx in 0..dataset.layer_count() {
        if let Ok(mut layer) = dataset.layer(layer_idx) {
            if layer.name().eq_ignore_ascii_case("M_COVR") {
                let mut geojson_parts: Vec<String> = Vec::new();

                for feature in layer.features() {
                    // Filter for CATCOV=1 (coverage available)
                    let catcov = feature
                        .field_index("CATCOV")
                        .ok()
                        .and_then(|idx| feature.field(idx).ok())
                        .flatten()
                        .and_then(|v| v.into_int());

                    if catcov != Some(1) {
                        continue;
                    }

                    if let Some(geom) = feature.geometry() {
                        match geom.json() {
                            Ok(json_str) if !json_str.is_empty() => {
                                geojson_parts.push(json_str);
                            }
                            Ok(_) => {
                                debug!("M_COVR feature has empty geometry");
                            }
                            Err(e) => {
                                warn!("Failed to get M_COVR geometry as GeoJSON: {}", e);
                            }
                        }
                    }
                }

                if geojson_parts.is_empty() {
                    return None;
                }

                // If single coverage polygon, return it directly
                if geojson_parts.len() == 1 {
                    return Some(geojson_parts.into_iter().next().unwrap());
                }

                // Multiple coverage polygons: wrap in a GeometryCollection
                let geometries = geojson_parts.join(",");
                return Some(format!(
                    r#"{{"type":"GeometryCollection","geometries":[{}]}}"#,
                    geometries
                ));
            }
        }
    }

    None
}

/// Find S-57 base files (.000) in an ENC directory.
/// Update files (.001+) are automatically applied by GDAL when UPDATES=APPLY is set.
pub fn find_s57_files(enc_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let entries = match fs::read_dir(enc_dir) {
        Ok(entries) => entries,
        Err(e) => {
            error!("Failed to read directory {:?}: {}", enc_dir, e);
            return files;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|v| v.to_str()) {
                // Only process base files (.000) - GDAL auto-applies updates
                if ext == "000" {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files
}

/// Find all ENC subdirectories under the given root
pub fn find_enc_directories(input_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let entries = match fs::read_dir(input_dir) {
        Ok(entries) => entries,
        Err(e) => {
            error!("Failed to read input directory {:?}: {}", input_dir, e);
            return dirs;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }

    dirs
}
