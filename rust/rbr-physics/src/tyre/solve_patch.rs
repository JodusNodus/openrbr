//! 0x535d60 `CScratch::SolvePatch`: one contact patch. When `this+0x3dc` is zero (or the compare is unordered: a
//! NaN takes the other path too) it clears the results. Otherwise it runs `StepInner` on the scratch's own record
//! (`this+0x20`), then two `TyreClamp` calls. The original is plain `ret`: it has no stack arguments.
//!
//! The asm's frame holds the `StepInner` vector and vec4 out (stack temporaries, not observable here). The
//! `StepInner` floats go straight to `this+0x3b8`, `+0x3e0` and `+0x3e4`. The second clamp's `b` is the dword at
//! `this+0x398`, pushed as is and read as a float, so it is a raw-bit read (listed in `README.md`).

use super::force::tyre_clamp;
use super::layout::{step_inner_input, Frame, Outside};
use super::step_inner::step_inner;
use crate::Variant;

/// 0x535d60 `CScratch::SolvePatch`. `Outside` when the `StepInner` search reads past its table.
pub fn solve_patch(f: &mut Frame, variant: Variant) -> Result<(), Outside> {
    if f.scratch.f3dc == 0.0 {
        let s = &mut f.scratch;
        s.f3b8 = 0.0;
        s.f3e8 = 0.0;
        s.f3ec = 0.0;
        s.f3e0 = 0.0;
        s.f3e4 = 0.0;
        s.x4c0 = 0.0;
        s.y4c4 = 0.0;
        return Ok(());
    }

    let out = step_inner(&step_inner_input(f)?, variant);
    f.scratch.f3b8 = out.arg3;
    f.scratch.f3e0 = out.arg4;
    f.scratch.f3e4 = out.arg5;

    let [x, y, _] = out.vec3;
    let (s, st) = (&f.scratch, &f.state);
    let clamp_x = tyre_clamp(s.nh + x, s.scale_3b0 * s.f394, st.k40);
    let clamp_y = tyre_clamp(st.radius * y + s.zero_350, s.f398, st.k44);
    f.scratch.f3e8 = clamp_x;
    f.scratch.f3ec = clamp_y;
    f.scratch.x4c0 = x;
    f.scratch.y4c4 = y;
    Ok(())
}
