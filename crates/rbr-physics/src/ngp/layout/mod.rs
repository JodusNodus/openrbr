//! NGP's memory side: what the model modules ([`super`]) read and write in the game's records and in NGP.dll's
//! `.data`. Addresses are NGP.dll's file addresses (base `0x10000000`; a running game relocates the DLL), the
//! exe's addresses and the records' offsets; the model modules know none of them.

pub mod atmosphere;
pub mod car_init;
pub mod drive_line;
pub mod material;
pub mod runtime;
pub mod stage;
pub mod tag_blend;
