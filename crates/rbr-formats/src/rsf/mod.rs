//! RallySimFans (RSF) installs: the community stages (`stages`) and the NGP car packs (`cars`), read in place, and
//! RSF's ghost cars (`ghosts`), the RBRHUD plugin's in-car gauges (`gauges`), a drive's force-feedback and wheel
//! settings (`settings`), and the Pacenote plugin's co-driver (`codriver`) and note files (`pacenotes`).

mod cars;
pub mod codriver;
pub mod gauges;
pub mod ghosts;
pub mod pacenotes;
pub mod settings;
mod stages;

pub use cars::*;
pub use stages::*;
