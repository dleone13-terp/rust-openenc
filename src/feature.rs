use gdal::vector::Feature;
use serde_json::{Map, Value};
use sqlx::{Postgres, Transaction};

use crate::s57::S57Metadata;
use crate::style::StyleLayerDef;

/// Common S-57 attributes shared across feature layers
#[derive(Debug)]
pub struct CommonAttributes {
    pub scamin: Option<f64>,
    pub objl: Option<i32>,
    pub sordat: Option<String>,
    pub sorind: Option<String>,
    pub other_attributes: Map<String, Value>,
}

/// Context for the current chart being processed
pub struct ChartContext<'a> {
    pub enc_name: &'a str,
    pub metadata: &'a S57Metadata,
}

/// Column type for layer-specific fields
#[derive(Clone, Copy)]
pub enum ColType {
    Float,
    Int,
    Text,
}

impl ColType {
    pub fn sql_type(&self) -> &'static str {
        match self {
            ColType::Float => "NUMERIC",
            ColType::Int => "INTEGER",
            ColType::Text => "TEXT",
        }
    }
}

/// Declarative column definition for a feature layer
pub struct ColumnDef {
    pub s57_field: &'static str,
    pub sql_column: &'static str,
    pub col_type: ColType,
}

impl ColumnDef {
    pub const fn new(s57_field: &'static str, sql_column: &'static str, col_type: ColType) -> Self {
        Self {
            s57_field,
            sql_column,
            col_type,
        }
    }
}

/// Style properties computed during import
#[derive(Debug, Default, Clone)]
pub struct StyleProps {
    pub ac: Option<String>, // area color token
    pub lc: Option<String>, // line color token
    pub sy: Option<String>, // point symbol name
}

/// Declarative layer definition — all you need to add a new S-57 feature layer
pub struct LayerDef {
    pub s57_name: &'static str,
    pub table: &'static str,
    pub columns: &'static [ColumnDef],
    pub style_fn: Option<fn(&Map<String, Value>) -> StyleProps>,
    pub style_layers: &'static [StyleLayerDef],
}

impl LayerDef {
    /// Generate `CREATE TABLE IF NOT EXISTS` DDL matching the standard column layout.
    pub fn create_table_sql(&self) -> String {
        let mut cols = String::new();
        cols.push_str("    id SERIAL PRIMARY KEY,\n");
        cols.push_str("    enc_name TEXT NOT NULL,\n");
        cols.push_str("    feature_fid INTEGER NOT NULL,\n");
        cols.push_str("    edition INTEGER,\n");
        cols.push_str("    update_number INTEGER DEFAULT 0,\n");
        cols.push_str("    compilation_scale INTEGER NOT NULL,\n");
        cols.push_str("    scamin NUMERIC,\n");
        cols.push_str("    objl INTEGER,\n");

        for col in self.columns {
            cols.push_str(&format!(
                "    {} {},\n",
                col.sql_column,
                col.col_type.sql_type()
            ));
        }

        cols.push_str("    ac TEXT,\n");
        cols.push_str("    lc TEXT,\n");
        cols.push_str("    sy TEXT,\n");
        cols.push_str("    sordat TEXT,\n");
        cols.push_str("    sorind TEXT,\n");
        cols.push_str("    attributes JSONB,\n");
        cols.push_str("    geom GEOMETRY(GEOMETRY, 4326),\n");
        cols.push_str("    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,\n");
        cols.push_str(&format!(
            "    CONSTRAINT {}_unique_feature UNIQUE (enc_name, edition, update_number, feature_fid)\n",
            self.table
        ));

        format!("CREATE TABLE IF NOT EXISTS {} (\n{});", self.table, cols)
    }

    /// Generate the 4 standard index DDL statements every layer table needs.
    pub fn create_indexes_sql(&self) -> Vec<String> {
        vec![
            format!(
                "CREATE INDEX IF NOT EXISTS {0}_geom_idx ON {0} USING GIST(geom);",
                self.table
            ),
            format!(
                "CREATE INDEX IF NOT EXISTS {0}_scamin_idx ON {0}(scamin) WHERE scamin IS NOT NULL;",
                self.table
            ),
            format!(
                "CREATE INDEX IF NOT EXISTS {0}_enc_name_idx ON {0}(enc_name);",
                self.table
            ),
            format!(
                "CREATE INDEX IF NOT EXISTS {0}_compilation_scale_idx ON {0}(compilation_scale);",
                self.table
            ),
        ]
    }

    /// Generate `CREATE OR REPLACE FUNCTION {table}_mvt(z, x, y)` PL/pgSQL function
    /// with tile envelope and njord-style ZFinder zoom filtering.
    pub fn create_mvt_function_sql(&self) -> String {
        let layer_select_cols: String = self
            .columns
            .iter()
            .map(|c| format!(",\n            d.{}", c.sql_column))
            .collect();

        format!(
            r#"CREATE OR REPLACE FUNCTION {table}_mvt(z integer, x integer, y integer, query_params json DEFAULT '{{}}'::json)
RETURNS bytea
AS $$
DECLARE
    mvt bytea;
    tile_env geometry;
    tile_env_4326 geometry;
BEGIN
    tile_env := ST_TileEnvelope(z, x, y);
    tile_env_4326 := ST_Transform(tile_env, 4326);

    SELECT INTO mvt ST_AsMVT(tile, '{table}', 4096, 'geom')
    FROM (
        SELECT
            ST_AsMVTGeom(
                ST_Transform(d.geom, 3857),
                tile_env,
                4096,
                64,
                true
            ) AS geom,
            d.id,
            d.enc_name,
            d.objl{layer_cols},
            d.ac AS "AC",
            d.lc AS "LC",
            d.sy AS "SY",
            d.scamin,
            d.sordat,
            d.attributes
        FROM {table} d
        WHERE
            d.geom && tile_env_4326
            AND ST_IsValid(d.geom)
            AND (28 - CEIL(LN(d.compilation_scale::double precision) / LN(2)))::int <= z
            AND (d.scamin IS NULL OR (28 - CEIL(LN(d.scamin::double precision) / LN(2)))::int <= z)
        ORDER BY d.compilation_scale DESC
    ) AS tile
    WHERE geom IS NOT NULL;

    RETURN mvt;
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;"#,
            table = self.table,
            layer_cols = layer_select_cols,
        )
    }
}

/// Runtime column value extracted from a feature
pub enum ColValue {
    Float(Option<f64>),
    Int(Option<i32>),
    Text(Option<String>),
}

/// Build the INSERT...ON CONFLICT upsert SQL for a layer definition.
///
/// Column order: enc_name, feature_fid, edition, update_number, compilation_scale,
/// scamin, objl, [layer-specific columns...], sordat, sorind, attributes, geom
pub fn build_upsert_sql(def: &LayerDef) -> String {
    let num_common_leading = 7; // enc_name, feature_fid, edition, update_number, compilation_scale, scamin, objl
    let num_layer = def.columns.len();
    let num_common_trailing = 7; // ac, lc, sy, sordat, sorind, attributes, geom
    let total = num_common_leading + num_layer + num_common_trailing;

    // Column names
    let layer_cols: Vec<&str> = def.columns.iter().map(|c| c.sql_column).collect();
    let all_cols = format!(
        "enc_name, feature_fid, edition, update_number, compilation_scale, scamin, objl, {}, ac, lc, sy, sordat, sorind, attributes, geom",
        layer_cols.join(", ")
    );

    // Placeholders $1..$N, with geom wrapped in ST_Force2D(ST_SetSRID(ST_GeomFromGeoJSON(...), 4326))
    // ST_Force2D strips Z coordinates to ensure 2D geometry (needed for SOUNDG features)
    let mut placeholders: Vec<String> = (1..total).map(|i| format!("${}", i)).collect();
    placeholders.push(format!(
        "ST_Force2D(ST_SetSRID(ST_GeomFromGeoJSON(${}), 4326))",
        total
    ));

    // ON CONFLICT update set — everything except the conflict key columns
    let mut update_parts: Vec<String> = vec![
        "compilation_scale = EXCLUDED.compilation_scale".to_string(),
        "scamin = EXCLUDED.scamin".to_string(),
        "objl = EXCLUDED.objl".to_string(),
    ];
    for col in &layer_cols {
        update_parts.push(format!("{} = EXCLUDED.{}", col, col));
    }
    update_parts.push("ac = EXCLUDED.ac".to_string());
    update_parts.push("lc = EXCLUDED.lc".to_string());
    update_parts.push("sy = EXCLUDED.sy".to_string());
    update_parts.push("sordat = EXCLUDED.sordat".to_string());
    update_parts.push("sorind = EXCLUDED.sorind".to_string());
    update_parts.push("attributes = EXCLUDED.attributes".to_string());
    update_parts.push("geom = EXCLUDED.geom".to_string());

    format!(
        "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT (enc_name, edition, update_number, feature_fid) DO UPDATE SET {}",
        def.table,
        all_cols,
        placeholders.join(", "),
        update_parts.join(", ")
    )
}

/// Extract layer-specific column values from the typed JSON map.
pub fn extract_values(def: &LayerDef, typed: &Map<String, Value>) -> Vec<ColValue> {
    def.columns
        .iter()
        .map(|col| {
            let val = typed.get(col.s57_field);
            match col.col_type {
                ColType::Float => ColValue::Float(val.and_then(|v| v.as_f64())),
                ColType::Int => ColValue::Int(val.and_then(|v| v.as_i64()).map(|v| v as i32)),
                ColType::Text => {
                    ColValue::Text(val.and_then(|v| v.as_str()).map(|s| s.to_string()))
                }
            }
        })
        .collect()
}

/// Generic upsert for any layer definition.
async fn upsert_feature(
    sql: &str,
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ChartContext<'_>,
    fid: i64,
    common: &CommonAttributes,
    col_values: &[ColValue],
    style: &StyleProps,
    geom_geojson: Option<&str>,
) -> Result<(), sqlx::Error> {
    let attributes_json = if common.other_attributes.is_empty() {
        None
    } else {
        Some(sqlx::types::Json(Value::Object(
            common.other_attributes.clone(),
        )))
    };

    // Bind common leading params
    let mut q = sqlx::query(sql)
        .bind(ctx.enc_name)
        .bind(fid)
        .bind(ctx.metadata.edition)
        .bind(ctx.metadata.update_number)
        .bind(ctx.metadata.compilation_scale)
        .bind(common.scamin)
        .bind(common.objl);

    // Bind layer-specific params
    for val in col_values {
        q = match val {
            ColValue::Float(v) => q.bind(*v),
            ColValue::Int(v) => q.bind(*v),
            ColValue::Text(v) => q.bind(v.as_deref()),
        };
    }

    // Bind style props
    q = q
        .bind(style.ac.as_deref())
        .bind(style.lc.as_deref())
        .bind(style.sy.as_deref());

    // Bind common trailing params
    q = q
        .bind(&common.sordat)
        .bind(&common.sorind)
        .bind(attributes_json)
        .bind(geom_geojson);

    q.execute(&mut **tx).await?;
    Ok(())
}

/// Helper: look up a field by name, returning None on missing/error.
fn get_field(feature: &Feature<'_>, name: &str) -> Option<gdal::vector::FieldValue> {
    let idx = feature.field_index(name).ok()?;
    feature.field(idx).ok().flatten()
}

/// Extract common S-57 attributes from a GDAL feature.
/// `known_fields` are layer-specific field names extracted into the typed map.
///
/// Uses individual field lookups by name instead of `feature.fields()` iterator
/// to avoid GDAL Rust binding panics on list-type fields with null pointers.
pub fn extract_common(
    feature: &Feature<'_>,
    known_fields: &[&str],
) -> (CommonAttributes, Map<String, Value>) {
    let mut typed = Map::new();

    let scamin = get_field(feature, "SCAMIN").and_then(|v| v.into_real());
    let objl = get_field(feature, "OBJL").and_then(|v| v.into_int());
    let sordat = get_field(feature, "SORDAT").and_then(|v| v.into_string());
    let sorind = get_field(feature, "SORIND").and_then(|v| v.into_string());

    for &field_name in known_fields {
        if let Some(fv) = get_field(feature, field_name) {
            if let Some(v) = crate::util::field_value_to_json(&fv) {
                typed.insert(field_name.to_uppercase(), v);
            }
        }
    }

    (
        CommonAttributes {
            scamin,
            objl,
            sordat,
            sorind,
            other_attributes: Map::new(),
        },
        typed,
    )
}

/// Process all features from a GDAL layer through a LayerDef
pub async fn process_layer(
    def: &LayerDef,
    dataset: &gdal::Dataset,
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ChartContext<'_>,
) -> Result<usize, Box<dyn std::error::Error>> {
    use gdal::vector::LayerAccess;
    use log::{debug, error, info, warn};

    let sql = build_upsert_sql(def);
    let s57_fields: Vec<&str> = def.columns.iter().map(|c| c.s57_field).collect();

    let mut count = 0;
    let mut error_count = 0;

    for layer_idx in 0..dataset.layer_count() {
        let mut layer = match dataset.layer(layer_idx) {
            Ok(l) => l,
            Err(_) => continue,
        };

        if !layer.name().eq_ignore_ascii_case(def.s57_name) {
            continue;
        }

        info!(
            "Processing {} layer with {} features",
            def.s57_name,
            layer.feature_count()
        );

        for feature in layer.features() {
            let fid = feature
                .fid()
                .and_then(|fid| i64::try_from(fid).ok())
                .unwrap_or(0);

            let geom_geojson = match feature.geometry() {
                Some(geom) => match geom.json() {
                    Ok(json_str) if !json_str.is_empty() => Some(json_str),
                    Ok(_) => {
                        debug!("Feature {} has empty geometry, skipping", fid);
                        continue;
                    }
                    Err(e) => {
                        warn!("Failed to convert geometry for feature {}: {}", fid, e);
                        error_count += 1;
                        continue;
                    }
                },
                None => {
                    debug!("Feature {} has no geometry, skipping", fid);
                    continue;
                }
            };

            let (common, typed) = extract_common(&feature, &s57_fields);
            let col_values = extract_values(def, &typed);
            let style = match def.style_fn {
                Some(f) => f(&typed),
                None => StyleProps::default(),
            };

            match upsert_feature(
                &sql,
                tx,
                ctx,
                fid,
                &common,
                &col_values,
                &style,
                geom_geojson.as_deref(),
            )
            .await
            {
                Ok(_) => count += 1,
                Err(e) => {
                    error!(
                        "Failed to upsert {} feature {}: {}",
                        def.s57_name, fid, e
                    );
                    error_count += 1;
                }
            }
        }
    }

    if error_count > 0 {
        warn!(
            "{}: {} features inserted, {} errors",
            def.s57_name, count, error_count
        );
    }

    Ok(count)
}
