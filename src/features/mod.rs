mod depare;
mod lights;
mod lndare;
mod soundg;

pub use depare::DEPARE;
pub use lights::LIGHTS;
pub use lndare::LNDARE;
pub use soundg::SOUNDG;

use crate::feature::LayerDef;

pub fn all_layers() -> &'static [&'static LayerDef] {
    &[&DEPARE, &LNDARE, &LIGHTS, &SOUNDG]
}
