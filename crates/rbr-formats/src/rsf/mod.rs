//! RallySimFans (RSF) installs: the community stages (`stages`) and the NGP car packs (`cars`), read in place, and
//! RSF's ghost cars (`ghosts`), the RBRHUD plugin's in-car gauges (`gauges`) and a drive's force-feedback and wheel
//! settings (`settings`).

mod cars;
pub mod gauges;
pub mod ghosts;
pub mod settings;
mod stages;

pub use cars::*;
pub use stages::*;
