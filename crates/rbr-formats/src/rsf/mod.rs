//! RallySimFans (RSF) installs: the community stages (`stages`) and the NGP car packs (`cars`), read in place, and
//! the RBRHUD plugin's in-car gauges (`gauges`), a drive's force-feedback and wheel
//! settings (`settings`), and the Pacenote plugin's co-driver (`codriver`) and note files (`pacenotes`).

mod cars;
pub mod codriver;
pub mod gauges;
mod json;
pub mod pacenotes;
pub mod settings;
mod stages;

pub use cars::*;
pub use stages::*;
