-- ENC catalog: one row per chart cell, stores compilation scale and coverage polygon
CREATE TABLE IF NOT EXISTS enc_catalog (
    enc_name TEXT PRIMARY KEY,
    compilation_scale INTEGER NOT NULL,
    edition INTEGER,
    update_number INTEGER,
    coverage GEOMETRY(GEOMETRY, 4326) NOT NULL
);

CREATE INDEX IF NOT EXISTS enc_catalog_coverage_idx ON enc_catalog USING GIST(coverage);
CREATE INDEX IF NOT EXISTS enc_catalog_scale_idx ON enc_catalog(compilation_scale);
