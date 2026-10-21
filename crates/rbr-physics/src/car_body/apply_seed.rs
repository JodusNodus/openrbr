//! The seed applier, `CCar::UpdateA720` (exe 0x4ea720, `ret 4`). It hands the seed's words to the drive (`M8770`), the
//! damage object (`NopD300`), the scratch sim (`CopyDef`) and the four wheels (`CopyCF30`), stores the seed's frame words
//! into the car's frame object, then runs UpdateMrr. The asm (not the C++) fixes the order. The seed, the wheel list and
//! the frame object are read where the asm reads them: after the calls before them, since a call can write memory. The
//! host loads them through `layout` each time (`SeedCalls::seed` and the others).

use super::layout::{self, Seed, FRAME_COPY};

/// The reads, calls and stores the seed applier makes, in the asm's order.
pub trait SeedCalls {
    /// The seed block as it is now (`layout::Seed::load`).
    fn seed(&mut self) -> Seed;
    /// The car's four wheel pointers as they are now (`layout::wheel_pointers`).
    fn wheels(&mut self) -> [u32; 4];
    /// The car's frame object as it is now (`layout::setup_target`).
    fn setup_target(&mut self) -> u32;
    /// An external call: `callee` on `this`, with `args` on the stack (the first one first).
    fn call(&mut self, callee: u32, this: u32, args: &[u32]);
    /// A word the seed applier stores (the frame words, at `at`).
    fn store_word(&mut self, at: u32, word: u32);
    /// `CCar::UpdateMrr` on the car.
    fn update_mrr(&mut self, car: u32);
}

/// `CDrive::M8770(src)` on the drive (`car + 0x4b0`).
const DRIVE_M8770: u32 = 0x508770;
/// `E41Plain::NopD300(p)` on the damage object (`car + 0x690`), which does nothing.
const NOP_D300: u32 = 0x50D300;
/// `CVehicleControlUnit::CScratchSim::CopyDef(src)` on the scratch sim (`car + 0x850`).
const COPY_DEF: u32 = 0x537C90;
/// `E51Wheel::CopyCF30(a, b, c)` on each wheel (`car + 0x470`, four slots).
const WHEEL_COPY_CF30: u32 = 0x53CF30;

/// `CCar::UpdateA720(seed)` (0x4ea720): the externals, the frame stores into the car's frame object, then UpdateMrr on
/// `car`. Each word is read right before the call or store that uses it.
pub fn apply_seed(calls: &mut impl SeedCalls, car: u32) {
    let drive = calls.seed().drive;
    calls.call(DRIVE_M8770, layout::drive_at(car), &[drive]);
    let damage = calls.seed().damage;
    calls.call(NOP_D300, layout::engine_at(car), &[damage]);
    let scratch = calls.seed().scratch;
    calls.call(COPY_DEF, layout::scratch_at(car), &[scratch]);
    for i in 0..4 {
        let wheel = calls.wheels()[i];
        let args = calls.seed().wheels[i];
        calls.call(WHEEL_COPY_CF30, wheel, &args);
    }
    let target = calls.setup_target();
    let frame = calls.seed().frame;
    for (&(at, _), word) in FRAME_COPY.iter().zip(frame) {
        calls.store_word(target + at, word);
    }
    calls.update_mrr(car);
}
