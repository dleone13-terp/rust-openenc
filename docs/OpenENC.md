# OpenENC

I wanted to maybe start talking about defining the specification for open source vector charts. I figured while I was porting over the njord logic I might as well try to document it. This can stay a work in progress.

## Styling

For the purposes of the architecture here, I want to have some of the styling defined within the data in the database. Maybe that is a bad idea. I want to define how something will be styled, and what properties that is based on. S-57 is split into layers (features) so that is how I will split up styling.

## GeoJSON Data (what's stored in PostGIS)

Same thing with the data. Define what fields are stored per feature.

## Chart Data

Specify how individual S-57 charts will be stored, currently stored as:
enc_name	text	
compilation_scale	integer	
edition	integer NULL	
update_number	integer NULL	
coverage	geometry(Geometry,4326)	

Compilation scale is used to compute the zoom level of specific features.

A feature is included in a tile when both:
- The chart's `compilation_scale` zoom level is `<= z` (the requested tile zoom)
- The feature's `scamin` zoom level (if set) is `<= z`

| compilation_scale | Min visible zoom |
|-------------------|-----------------|
| 1,000,000         | 8               |
| 500,000           | 9               |
| 200,000           | 10              |
| 100,000           | 11              |
| 50,000            | 12              |
| 25,000            | 13              |
| 10,000            | 14              |
