//! NGP 7.4.778 (Next Generation Physics): its physics functions, ported (`notes/ngp.md`). NGP is the 1.02 physics plus
//! these functions, repointed constants and code edits in exe functions, and per-car values in its `.data`.
//! [`layout`] is the memory side: NGP.dll's addresses, the game's records, the per-car init and the stage state.
//!
//! NGP's own code is SSE: every operation rounds to its type, unlike the exe's x87 code, which runs at 53-bit
//! precision under NGP (`notes/ngp.md`, "x87 precision").

pub mod atmosphere;
pub mod axle;
pub mod brake;
pub mod diff;
pub mod downforce;
pub mod drive;
pub mod drive_line;
pub mod engine;
pub mod exe_patches;
pub mod ffb;
pub mod heat;
pub mod inclination;
pub mod layout;
pub mod material;
pub mod outputs;
pub mod spring;
pub mod stage_materials;
pub mod starter;
pub mod step_wrapper;
pub mod tag_blend;
pub mod tyre_types;

/// NGP's sign: 0 for a zero, else -1 or 1 by the sign bit (NaN too, unlike 1.02's 0).
pub fn sign(x: f32) -> i32 {
    if x == 0.0 {
        0
    } else if x.is_sign_negative() {
        -1
    } else {
        1
    }
}
