use log::info;
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::feature::LayerDef;
use crate::s57::S57Metadata;

pub async fn create_pool(db_url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
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
            .unwrap_or_else(|e| {
                panic!("Failed to create MVT function for {}: {}", def.table, e)
            });

        info!("Ensured schema for table: {}", def.table);
    }
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
