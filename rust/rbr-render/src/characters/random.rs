//! The game's two random number generators, as the scripted events and the character loader draw them.
//!
//! - [`CrtRand`]: the C library's `rand()` (msvcr71, the exe's `/MD` import at `0x729540`): the 32-bit LCG
//!   `x = x * 214013 + 2531011`, returning bits 16-30. `PickRand` (0x445c70), `CRescueHelicopter::Activate`
//!   (0x49bc90, the side roll), `CPC_RescueHelicopter::Vf18` (the heading) and `Burst` (0x49bf90) draw from it.
//! - [`Lua`]: `LuaRandom(lo, hi)` (`Lua_LuaRandom` 0x43be90 -> `E01_RandomRange` 0x43bdb0): the exe's own LCG
//!   `g_e01Seed = g_e01Seed * 0x2000081 + 0xe623f41d`, `lo + seed % (hi - lo + 1)`. While a group of kind 2 loads
//!   (`E01RandSelect` 0x43bce0, from the stage loader 0x445eb0), a value already drawn in the group is skipped:
//!   the seed counts up until the value is new, until the group has drawn every value of the range. Any other item
//!   clears the set (`E01RandClear` 0x43bd30).
//!
//! Both are seeded with the same `time(NULL)` when a race starts (`E59Host90::M00567640`: `E01SetSeed` and `srand`,
//! the seed kept at `+0x1783c` for the replay, which seeds both from it again). The 2D effect system's own copy of the
//! LCG ([`Lcg::windscreen`]) draws the bird strike's windscreen point (`RandPair` 0x5f71e0).
//!
//! Faithful: the generators, the seeding and the exclusion. Ours: the order. The harness snapshots show 9 `LuaRandom`
//! draws and 882 `rand()` calls between the race seed and the first step on track-54_E, then 4 to 45 `rand()` calls a
//! step that differ from run to run (`notes/scripted-events.md`, seed order): the stage items mostly draw before the
//! race seed, and the race-time draws depend on every other caller. Not known: the 2D system's seed.

pub use rbr_gameplay::crt::CrtRand;

/// The exe's LCG step (`E01_RandomRange`, `RandPair`).
fn step(seed: u32) -> u32 {
    seed.wrapping_mul(0x0200_0081).wrapping_add(0xe623_f41d)
}

/// `LuaRandom`'s state: `g_e01Seed`, and the values drawn in the kind-2 group loading now (`g_e01RandFlag`,
/// `g_e01RandVec`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lua {
    seed: u32,
    group: u32,
    drawn: Vec<i32>,
}

impl Lua {
    /// `E01SetSeed(seed)`.
    pub fn new(seed: u32) -> Lua {
        Lua { seed, group: 0, drawn: Vec::new() }
    }

    /// `E01RandSelect(group)`: a new group starts an empty set; the same group keeps its own.
    pub fn select(&mut self, group: u32) {
        if group != self.group {
            self.drawn.clear();
            self.group = group;
        }
    }

    /// `E01RandClear`.
    pub fn clear(&mut self) {
        self.drawn.clear();
        self.group = 0;
    }

    /// `E01_RandomRange(lo, hi)`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        self.seed = step(self.seed);
        let span = (hi.wrapping_sub(lo) as u32).wrapping_add(1);
        if span == 0 {
            return lo;
        }
        let mut value = (self.seed % span) as i32 + lo;
        if self.group != 0 && (self.drawn.len() as u32) < span {
            // The scan starts over after each bump, so the value ends past every one drawn.
            let mut bumped = self.seed;
            let mut k = 0;
            while k < self.drawn.len() {
                if self.drawn[k] == value {
                    bumped = bumped.wrapping_add(1);
                    value = (bumped % span) as i32 + lo;
                    k = 0;
                }
                k += 1;
            }
            self.drawn.push(value);
        }
        value
    }
}

/// The 2D effect system's LCG (`NParticleSystem2D +0x44`, the exe's step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lcg(u32);

impl Lcg {
    pub fn new(seed: u32) -> Lcg {
        Lcg(seed)
    }

    /// `RandPair` (0x5f71e0): two draws of `seed % 0x7fff / 32768`, as the windscreen point's fractions `(x, y)` of
    /// its chart, before [`windscreen_point`] scales them.
    pub fn windscreen(&mut self) -> (f32, f32) {
        let a = step(self.0);
        let b = step(a);
        self.0 = b;
        let unit = |u: u32| (u % 0x7fff) as f32 * (1.0 / 32768.0);
        (unit(a), unit(b))
    }
}

/// `RandPair`'s point on the windscreen chart (`+0x180`): `x = r * ratio * 0.8 + 0.1`, `y = r * ratio * 0.4 + 0.3`
/// (constants 0x72f610, 0x72b068, 0x72f664, 0x72afcc), `ratio` being the chart's `+0x30` (`SetupBig` 0x5f6930: the
/// bottom edge's length over the side edge's).
pub fn windscreen_point((rx, ry): (f32, f32), ratio: f32) -> (f32, f32) {
    (rx * ratio * 0.8 + 0.1, ry * ratio * 0.4 + 0.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rand_is_the_c_librarys() {
        // msvcrt with the default seed 1: 41, 18467, 6334, 26500.
        let mut crt = CrtRand::srand(1);
        assert_eq!([crt.draw(), crt.draw(), crt.draw(), crt.draw()], [41, 18467, 6334, 26500]);
    }

    #[test]
    fn lua_random_steps_the_exes_lcg_and_skips_values_drawn_in_a_group() {
        let mut lua = Lua::new(0);
        // 0 * 0x2000081 + 0xe623f41d = 3861115933; % 4 = 1.
        assert_eq!(lua.range(1, 4), 2);
        let mut free = Lua::new(7);
        let mut grouped = Lua::new(7);
        grouped.select(34287);
        let a: Vec<i32> = (0..4).map(|_| free.range(1, 4)).collect();
        let b: Vec<i32> = (0..4).map(|_| grouped.range(1, 4)).collect();
        let mut sorted = b.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, [1, 2, 3, 4], "{a:?} {b:?}");
        // A full set draws freely again; clearing starts over.
        assert!((1..=4).contains(&grouped.range(1, 4)));
        grouped.clear();
        assert_eq!(grouped.drawn.len(), 0);
    }

    #[test]
    fn the_windscreen_point_is_in_the_charts_lower_middle() {
        let mut lcg = Lcg::new(12345);
        for _ in 0..100 {
            let (x, y) = windscreen_point(lcg.windscreen(), 1.0);
            assert!((0.1..0.9).contains(&x) && (0.3..0.7).contains(&y), "{x} {y}");
        }
    }
}
