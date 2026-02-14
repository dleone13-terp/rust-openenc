use log::info;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

use crate::feature::LayerDef;
use crate::s57::S57Metadata;

pub async fn create_pool(db_url: &str, max_connections: u32, min_connections: u32) -> PgPool {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(min_connections)
        .acquire_timeout(Duration::from_secs(30)) // Allow time for connection acquisition
        .idle_timeout(Duration::from_secs(600)) // 10 minutes idle timeout
        .max_lifetime(Duration::from_secs(1800)) // 30 minutes max lifetime
        .connect(db_url)
        .await
        .expect("Failed to connect to database")
}

pub async fn run_migrations(pool: &PgPool) {
    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
    info!("Migrations complete");
}

/// Create tables, indexes, and MVT functions for all registered layers.
/// Uses idempotent DDL so it's a no-op for existing tables.
pub async fn ensure_layer_tables(pool: &PgPool) {
    let layers = crate::features::all_layers();
    for def in layers {
        sqlx::query(&def.create_table_sql())
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("Failed to create table {}: {}", def.table, e));

        for sql in def.create_indexes_sql() {
            sqlx::query(&sql)
                .execute(pool)
                .await
                .unwrap_or_else(|e| panic!("Failed to create index for {}: {}", def.table, e));
        }

        sqlx::query(&def.create_mvt_function_sql())
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("Failed to create MVT function for {}: {}", def.table, e));

        info!("Ensured schema for table: {}", def.table);
    }

    // Create unified MVT function that combines all layers
    let unified_mvt_sql = create_unified_mvt_function_sql(&layers);
    sqlx::query(&unified_mvt_sql)
        .execute(pool)
        .await
        .expect("Failed to create unified MVT function");

    info!("Created unified enc_mvt function");
}

/// Generate a unified MVT function that combines all feature layers into a single source
fn create_unified_mvt_function_sql(layers: &[&LayerDef]) -> String {
    let layer_mvts: Vec<String> = layers
        .iter()
        .map(|def| {
            // Build layer-specific column list
            let layer_select_cols: String = def
                .columns
                .iter()
                .map(|c| format!(",\n                d.{}", c.sql_column))
                .collect();

            // Special handling for soundg: add depth unit conversions
            let depth_conversions = if def.table == "soundg" {
                r#",
                FLOOR(d.depth)::INTEGER AS depth_meters_whole,
                FLOOR((d.depth - FLOOR(d.depth)) * 10)::INTEGER AS depth_meters_tenths,
                ROUND(d.depth * 3.28084)::INTEGER AS depth_feet,
                FLOOR(d.depth / 1.8288)::INTEGER AS depth_fathoms,
                ROUND((d.depth / 1.8288 - FLOOR(d.depth / 1.8288)) * 6)::INTEGER AS depth_fathoms_feet"#
            } else {
                ""
            };

            format!(
                r#"COALESCE((SELECT ST_AsMVT(tile, '{table}', 4096, 'geom')
        FROM (
            SELECT
                ST_AsMVTGeom(
                    d.geom_3857,
                    tile_env,
                    4096,
                    128,
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
                d.attributes{depth_conv}
            FROM {table} d
            WHERE
                d.geom && tile_env_4326
                AND d.geom_3857 IS NOT NULL
                AND d.min_zoom <= z
                AND (d.max_zoom IS NULL OR d.max_zoom <= z)
            ORDER BY d.compilation_scale DESC
        ) AS tile
        WHERE geom IS NOT NULL), ''::bytea)"#,
                table = def.table,
                layer_cols = layer_select_cols,
                depth_conv = depth_conversions,
            )
        })
        .collect();

    let mvt_concatenation = layer_mvts.join("\n    || ");

    format!(
        r#"CREATE OR REPLACE FUNCTION enc_mvt(z integer, x integer, y integer, query_params json DEFAULT '{{}}'::json)
RETURNS bytea
AS $$
DECLARE
    mvt bytea;
    tile_env geometry;
    tile_env_4326 geometry;
BEGIN
    tile_env := ST_TileEnvelope(z, x, y);
    tile_env_4326 := ST_Transform(tile_env, 4326);

    SELECT INTO mvt
    {}
    ;

    RETURN mvt;
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;"#,
        mvt_concatenation
    )
}

/// Insert or update enc_catalog row for a chart cell.
/// If coverage_geojson is None, inserts a placeholder point at 0,0 that will
/// be updated later with a convex hull fallback.
pub async fn upsert_enc_catalog(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    enc_name: &str,
    metadata: &S57Metadata,
    coverage_geojson: Option<&str>,
) -> Result<(), sqlx::Error> {
    match coverage_geojson {
        Some(geojson) => {
            sqlx::query(
                r#"
                INSERT INTO enc_catalog (enc_name, compilation_scale, edition, update_number, coverage)
                VALUES ($1, $2, $3, $4, ST_SetSRID(ST_GeomFromGeoJSON($5), 4326))
                ON CONFLICT (enc_name) DO UPDATE SET
                    compilation_scale = EXCLUDED.compilation_scale,
                    edition = EXCLUDED.edition,
                    update_number = EXCLUDED.update_number,
                    coverage = EXCLUDED.coverage
                "#,
            )
            .bind(enc_name)
            .bind(metadata.compilation_scale)
            .bind(metadata.edition)
            .bind(metadata.update_number)
            .bind(geojson)
            .execute(&mut **tx)
            .await?;
        }
        None => {
            // Insert with a dummy point; will be replaced by convex hull fallback
            sqlx::query(
                r#"
                INSERT INTO enc_catalog (enc_name, compilation_scale, edition, update_number, coverage)
                VALUES ($1, $2, $3, $4, ST_SetSRID(ST_MakePoint(0, 0), 4326))
                ON CONFLICT (enc_name) DO UPDATE SET
                    compilation_scale = EXCLUDED.compilation_scale,
                    edition = EXCLUDED.edition,
                    update_number = EXCLUDED.update_number
                "#,
            )
            .bind(enc_name)
            .bind(metadata.compilation_scale)
            .bind(metadata.edition)
            .bind(metadata.update_number)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// Update enc_catalog coverage from convex hull of all features when M_COVR was missing
pub async fn update_catalog_coverage_fallback(
    pool: &PgPool,
    enc_name: &str,
    layers: &[&LayerDef],
) -> Result<(), sqlx::Error> {
    let subqueries: Vec<String> = layers
        .iter()
        .map(|def| {
            format!(
                "(SELECT ST_ConvexHull(ST_Collect(geom)) FROM {} WHERE enc_name = $1)",
                def.table
            )
        })
        .collect();

    let sql = format!(
        "UPDATE enc_catalog SET coverage = COALESCE({}, coverage) WHERE enc_name = $1 AND ST_Equals(coverage, ST_SetSRID(ST_MakePoint(0, 0), 4326))",
        subqueries.join(", ")
    );

    sqlx::query(&sql).bind(enc_name).execute(pool).await?;
    Ok(())
}

/// Check if an ENC is already imported with the same edition and update number
pub async fn is_enc_already_imported(
    pool: &PgPool,
    enc_name: &str,
    edition: i32,
    update_number: i32,
) -> Result<bool, sqlx::Error> {
    let result: Option<(i32, i32)> =
        sqlx::query_as("SELECT edition, update_number FROM enc_catalog WHERE enc_name = $1")
            .bind(enc_name)
            .fetch_optional(pool)
            .await?;

    match result {
        Some((existing_edition, existing_update)) => {
            Ok(existing_edition == edition && existing_update == update_number)
        }
        None => Ok(false),
    }
}
