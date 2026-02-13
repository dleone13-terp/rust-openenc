mod db;
mod feature;
mod features;
mod s57;
mod sprite;
mod style;
mod util;

use clap::Parser;
use gdal::Dataset;
use gdal::version::VersionInfo;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error, info, warn};

use std::env;
use std::path::PathBuf;

use feature::LayerDef;

/// Initialize GDAL with S-57 specific options
fn init_gdal() {
    // Configure GDAL S-57 driver options
    // ADD_SOUNDG_DEPTH=ON - automatically adds depth from Z coordinates as DEPTH field
    // SPLIT_MULTIPOINT=ON - splits multipoint soundings into individual points
    // See: https://gdal.org/drivers/vector/s57.html
    gdal::config::set_config_option(
        "OGR_S57_OPTIONS",
        "RETURN_PRIMITIVES=OFF,RETURN_LINKAGES=OFF,LNAM_REFS=ON,UPDATES=APPLY,SPLIT_MULTIPOINT=ON,RECODE_BY_DSSI=ON,ADD_SOUNDG_DEPTH=ON"
    ).expect("Failed to set OGR_S57_OPTIONS");
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, required_unless_present_any = ["style_output", "sprites_output"])]
    input_dir: Option<PathBuf>,

    #[arg(long, default_value = "info")]
    log_level: String,

    /// Write Mapbox GL style JSON to this path and exit
    #[arg(long)]
    style_output: Option<PathBuf>,

    /// Generate themed sprite SVGs into this directory and exit
    #[arg(long)]
    sprites_output: Option<PathBuf>,

    /// Color theme for style generation
    #[arg(long, default_value = "day")]
    theme: String,

    /// Vector tile source URL for style JSON
    #[arg(long, default_value = "http://localhost:3000")]
    tile_source_url: String,
}

/// Process a single S-57 file
async fn process_s57_file(
    s57_path: &PathBuf,
    pool: &sqlx::PgPool,
    layers: &[&LayerDef],
) -> Result<usize, Box<dyn std::error::Error>> {
    let enc_name = util::enc_name_from_path(s57_path);
    info!("Processing S-57 file: {} ({})", s57_path.display(), enc_name);

    let dataset = Dataset::open(s57_path)?;
    let metadata = s57::extract_metadata(&dataset);

    debug!(
        "S-57 metadata: edition={:?}, update={}, compilation_scale={}",
        metadata.edition, metadata.update_number, metadata.compilation_scale
    );

    // Extract M_COVR coverage polygon
    let coverage_geojson = s57::extract_coverage_geojson(&dataset);
    let has_coverage = coverage_geojson.is_some();

    // Begin transaction
    let mut tx = pool.begin().await?;

    // Upsert enc_catalog
    db::upsert_enc_catalog(&mut tx, &enc_name, &metadata, coverage_geojson.as_deref()).await?;

    // Process each feature layer
    let ctx = feature::ChartContext {
        enc_name: &enc_name,
        metadata: &metadata,
    };

    let mut total_count = 0;
    for layer_def in layers {
        match feature::process_layer(layer_def, &dataset, &mut tx, &ctx).await {
            Ok(count) => {
                if count > 0 {
                    info!("{}: {} features inserted for {}", layer_def.s57_name, count, enc_name);
                }
                total_count += count;
            }
            Err(e) => {
                error!("Failed processing {} for {}: {}", layer_def.s57_name, enc_name, e);
            }
        }
    }

    tx.commit().await?;

    // If M_COVR was missing, update coverage from convex hull of inserted features
    if !has_coverage && total_count > 0 {
        debug!("No M_COVR for {}, computing coverage from feature convex hull", enc_name);
        if let Err(e) = db::update_catalog_coverage_fallback(pool, &enc_name, layers).await {
            warn!("Failed to update coverage fallback for {}: {}", enc_name, e);
        }
    }

    info!("Completed {}: {} total features", enc_name, total_count);
    Ok(total_count)
}

/// Process an ENC directory containing S-57 base file and updates
async fn process_enc_directory(
    enc_dir: &PathBuf,
    pool: &sqlx::PgPool,
    layers: &[&LayerDef],
) {
    debug!("Scanning ENC directory: {:?}", enc_dir);

    let s57_files = s57::find_s57_files(enc_dir);

    if s57_files.is_empty() {
        warn!("No S-57 files found in {:?}", enc_dir);
        return;
    }

    info!("Found {} S-57 files in {:?}", s57_files.len(), enc_dir);

    for s57_path in s57_files {
        match process_s57_file(&s57_path, pool, layers).await {
            Ok(count) => {
                debug!("Processed {} with {} features", s57_path.display(), count);
            }
            Err(e) => {
                error!("Failed to process {:?}: {}", s57_path, e);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&args.log_level))
        .init();

    // Initialize GDAL with S-57 options
    init_gdal();

    // Sprite generation mode — no DB or GDAL needed
    if let Some(sprites_output) = &args.sprites_output {
        let svg_source = PathBuf::from("sprites/svg");
        sprite::generate_themed_sprites(&svg_source, sprites_output);
        info!("Generated themed sprites in {:?}", sprites_output);
        return;
    }

    // Style JSON generation mode — no DB or GDAL needed
    if let Some(style_path) = &args.style_output {
        let layers = features::all_layers();
        let json = style::generate_style_json(layers, &args.theme, &args.tile_source_url);
        std::fs::write(style_path, json).expect("Failed to write style JSON");
        info!("Wrote style JSON to {:?}", style_path);
        return;
    }

    let input_dir = args.input_dir.as_ref().expect("--input-dir is required");

    info!("GDAL version: {}", VersionInfo::version_summary());
    info!("Input directory: {:?}", input_dir);

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    info!("Using database URL: {}", db_url);
    let pool = db::create_pool(&db_url).await;

    db::run_migrations(&pool).await;
    db::ensure_layer_tables(&pool).await;

    let layers = features::all_layers();
    let enc_paths = s57::find_enc_directories(input_dir);
    info!("Found {} ENC directories", enc_paths.len());

    let pb = ProgressBar::new(enc_paths.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message("Processing ENCs");

    for enc_dir in &enc_paths {
        process_enc_directory(enc_dir, &pool, layers).await;
        pb.inc(1);
    }
    pb.finish_with_message("Done processing ENCs");
}
