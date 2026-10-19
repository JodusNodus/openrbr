//! RallySimFans (RSF) installs: the community stages (`stages`) and the NGP car packs (`cars`), read in place, and
//! RSF's ghost cars (`ghosts`), and the RBRHUD plugin's in-car gauges (`gauges`).

mod cars;
pub mod gauges;
pub mod ghosts;
mod stages;

pub use cars::*;
pub use stages::*;
