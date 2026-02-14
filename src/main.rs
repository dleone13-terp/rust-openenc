mod colors;
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
use std::sync::Arc;
use tokio::sync::Semaphore;

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

    // Configure polygon organization method to avoid slow processing of complex polygons
    // ONLY_CCW: Assume clockwise = outer ring, counter-clockwise = holes (fast)
    // This is safe for S-57 data which follows standard ring orientation conventions
    // See: https://gdal.org/en/latest/user/configoptions.html#vector-related-options
    gdal::config::set_config_option("OGR_ORGANIZE_POLYGONS", "ONLY_CCW")
        .expect("Failed to set OGR_ORGANIZE_POLYGONS");
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

    /// Force reimport of ENCs even if already present with same edition/update
    #[arg(long, default_value_t = false)]
    force_reimport: bool,

    /// Maximum number of database connections in the pool
    #[arg(long, default_value_t = 20)]
    max_connections: u32,

    /// Minimum number of database connections to keep warm
    #[arg(long, default_value_t = 5)]
    min_connections: u32,

    /// Number of ENCs to process in parallel
    #[arg(long, default_value_t = 10)]
    parallel_enc: usize,
}

/// Process a single S-57 file
async fn process_s57_file(
    s57_path: &PathBuf,
    pool: &sqlx::PgPool,
    layers: &[&LayerDef],
    force_reimport: bool,
) -> Result<usize, Box<dyn std::error::Error>> {
    let enc_name = util::enc_name_from_path(s57_path);
    info!(
        "Processing S-57 file: {} ({})",
        s57_path.display(),
        enc_name
    );

    let dataset = Dataset::open(s57_path)?;
    let metadata = s57::extract_metadata(&dataset);

    debug!(
        "S-57 metadata: edition={:?}, update={}, compilation_scale={}",
        metadata.edition, metadata.update_number, metadata.compilation_scale
    );

    // Skip if already imported with same edition/update (unless force_reimport is enabled)
    if !force_reimport {
        match db::is_enc_already_imported(
            pool,
            &enc_name,
            metadata.edition.unwrap_or(0),
            metadata.update_number,
        )
        .await
        {
            Ok(true) => {
                info!(
                    "Skipping {} - already imported with same edition/update",
                    enc_name
                );
                return Ok(0);
            }
            Ok(false) => {}
            Err(e) => {
                warn!("Failed to check if {} is already imported: {}", enc_name, e);
            }
        }
    }

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
                    info!(
                        "{}: {} features inserted for {}",
                        layer_def.s57_name, count, enc_name
                    );
                }
                total_count += count;
            }
            Err(e) => {
                error!(
                    "Failed processing {} for {}: {}",
                    layer_def.s57_name, enc_name, e
                );
            }
        }
    }

    tx.commit().await?;

    // If M_COVR was missing, update coverage from convex hull of inserted features
    if !has_coverage && total_count > 0 {
        debug!(
            "No M_COVR for {}, computing coverage from feature convex hull",
            enc_name
        );
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
    force_reimport: bool,
) {
    debug!("Scanning ENC directory: {:?}", enc_dir);

    let s57_files = s57::find_s57_files(enc_dir);

    if s57_files.is_empty() {
        warn!("No S-57 files found in {:?}", enc_dir);
        return;
    }

    info!("Found {} S-57 files in {:?}", s57_files.len(), enc_dir);

    for s57_path in s57_files {
        match process_s57_file(&s57_path, pool, layers, force_reimport).await {
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
    info!(
        "Database pool: max={}, min={}",
        args.max_connections, args.min_connections
    );
    let pool = db::create_pool(&db_url, args.max_connections, args.min_connections).await;

    db::run_migrations(&pool).await;
    db::ensure_layer_tables(&pool).await;

    let layers = features::all_layers();
    let enc_paths = s57::find_enc_directories(input_dir);
    info!("Found {} ENC directories", enc_paths.len());

    let pb = Arc::new(ProgressBar::new(enc_paths.len() as u64));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message("Processing ENCs");

    // Limit concurrent ENC processing to avoid exhausting database connections
    info!("Processing ENCs with parallelism={}", args.parallel_enc);
    let semaphore = Arc::new(Semaphore::new(args.parallel_enc));
    let mut tasks = Vec::new();
    let force_reimport = args.force_reimport;

    for enc_dir in enc_paths {
        let pool = pool.clone();
        let pb = Arc::clone(&pb);
        let semaphore = Arc::clone(&semaphore);

        // Use spawn_blocking since GDAL Dataset is not Send
        let task = tokio::task::spawn_blocking(move || {
            // Create a new tokio runtime for async operations within the blocking task
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move {
                // Acquire semaphore permit to limit concurrency
                let _permit = semaphore.acquire().await.unwrap();

                process_enc_directory(&enc_dir, &pool, layers, force_reimport).await;
                pb.inc(1);
            })
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    pb.finish_with_message("Done processing ENCs");
}
