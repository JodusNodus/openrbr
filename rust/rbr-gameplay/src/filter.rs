//! The car-control input filter: the raw control channels to the filtered channels the quantizer packs
//! (`E16Filter::Tic` 0x48c500, `Step` 0x48af90, `StepB` 0x48b1d0). Each channel has a response cell chosen by a
//! selector the input poll sets, with a curve, a rise and fall rate limit, and a speed fade (`m_36c`, [`notify_a`]). Packing is in [`crate::control`]. The order of the work and what is checked is in
//! `README.md`; the arithmetic is the asm's (f32 at each operation, the order of the operands as compiled).

use crate::ini::Ini;

/// Channels: 0 steering (raw +0x14), then 1 to 4 (raw +0x18, +0x1c, +0x20, +0x24).
pub const CHANNELS: usize = 5;
const CELLS: usize = 4;

/// One response cell (0x24 bytes in the grid; word names as the asm reads them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    /// Word 0: the channel is filtered at all (`Active`; `Grid::reset` recomputes it from the values).
    pub active: bool,
    /// Word 1: the curve is applied (`Parabolic`; `Grid::reset` sets it when the linear term is not 1).
    pub nonlinear: bool,
    /// Word 2: the linear term of the curve.
    pub lin: f32,
    /// Word 3: the quadratic term.
    pub c2: f32,
    /// Word 4: the cubic term.
    pub c3: f32,
    /// Word 5: the rise rate per second, against the output (`RiseRate`).
    pub rise: f32,
    /// Word 6: the fall rate per second (`FallRate`).
    pub fall: f32,
    /// Word 7: the speed fade's amount (`MaxSpeedFade`).
    pub fade: f32,
    /// Word 8: the speed at which the fade is full (`MaxSpeedFadeSpeed`).
    pub fade_speed: f32,
}

impl Cell {
    /// `E16_InitGrid` (0x48bb70): zero, with the linear term 1.0.
    const INITIAL: Self = Self {
        active: false,
        nonlinear: false,
        lin: 1.0,
        c2: 0.0,
        c3: 0.0,
        rise: 0.0,
        fall: 0.0,
        fade: 0.0,
        fade_speed: 0.0,
    };
}

/// The grid: per channel, the selector (the cell `Step` uses) and four cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub cells: [[Cell; CELLS]; CHANNELS],
    /// The cell per channel. The input poll sets it (`NotifyB`); `Grid::reset` sets it to 0.
    pub select: [u32; CHANNELS],
}

impl Grid {
    /// The grid a quick race starts from with the INI settings and no saved grid: `E16_InitGrid` and `E16_LoadINI`
    /// (0x48b310, which fills steering cells 0, 1 and 3 from `PC_InputFilter.ini`), then `E16_Reset` as the quick race
    /// set-up calls it (`StartQuickRace`, 0x481660). The saved grid of a profile (`E16Filter::ReadBin`, 0x48bd60) is
    /// not read here.
    pub fn from_settings(ini: &Ini) -> Self {
        let mut grid = Self { cells: [[Cell::INITIAL; CELLS]; CHANNELS], select: [0; CHANNELS] };
        let cells = &mut grid.cells[0];
        let (axis, stick, button) = ("STEER::AXIS", "STEER::STICK", "STEER::BUTTON_DIGITAL");
        cells[0].active = ini.bool(axis, "Active", true);
        cells[0].nonlinear = ini.bool(axis, "Parabolic", true);
        cells[0].fade = ini.float(axis, "MaxSpeedFade", 0.3);
        cells[0].fade_speed = ini.float(axis, "MaxSpeedFadeSpeed", 35.0);
        cells[1].active = ini.bool(stick, "Active", true);
        cells[1].nonlinear = ini.bool(stick, "Parabolic", true);
        cells[1].fade = ini.float(stick, "MaxSpeedFade", 0.5);
        cells[1].fade_speed = ini.float(stick, "MaxSpeedFadeSpeed", 27.0);
        cells[3].active = ini.bool(button, "Active", true);
        cells[3].rise = ini.float(button, "RiseRate", 0.3);
        cells[3].fall = ini.float(button, "FallRate", 0.5);
        cells[3].fade = ini.float(button, "MaxSpeedFade", 0.5);
        cells[3].fade_speed = ini.float(button, "MaxSpeedFadeSpeed", 27.0);
        grid.reset();
        grid
    }

    /// `E16_Reset` (0x48ac60) on the cells: `active` when the cubic or quadratic term, a rate or the fade is set;
    /// `nonlinear` when the linear term is not 1.0. The selectors go to 0.
    pub fn reset(&mut self) {
        self.select = [0; CHANNELS];
        for cell in self.cells.iter_mut().flatten() {
            cell.active = cell.c2 != 0.0 || cell.c3 != 0.0 || cell.rise != 0.0 || cell.fall != 0.0 || cell.fade != 0.0;
            cell.nonlinear = cell.lin != 1.0;
        }
    }

    /// The cell a channel's `Step` uses.
    pub fn cell(&self, channel: usize) -> &Cell {
        &self.cells[channel][self.select[channel] as usize]
    }
}

/// `E16_RespB` (0x48a4e0): the speed scale for a speed `v`, the fade's amount `amount` below the fade speed `b`.
pub fn speed_scale(v: f32, amount: f32, fade_speed: f32) -> f32 {
    if v < fade_speed {
        1.0 - (v / fade_speed) * amount
    } else {
        1.0 - amount
    }
}

/// `sin(1.0471976f)` of `NotifyA` (`__CIsin`, 60 degrees): the x87 result rounded to f32.
fn sin_60() -> f32 {
    (std::f32::consts::FRAC_PI_3 as f64).sin() as f32
}

/// `E16Filter::NotifyA` (0x48aa10): the speed fade input `m_36c` from `CSigCarToCarInputFilter`'s payload: `speed`
/// (word 0, the car's forward speed `-MB560[1]` of `CCar+0x15f8`, clamped at 0 by the emitter) and `vector` (words
/// 4..8, `MB560` of the car's vector at +0x220). The vector's lane 2 is cleared and `CFloat3::Length` of it must exceed
/// 0.01; it is scaled to unit length; when its y is negative, the speed is scaled by `1 - min(|x / sin 60|, 1)`;
/// otherwise the input is 0. The payload's second vector (`GetDir`) is stored by the original and not read here.
pub fn notify_a(speed: f32, vector: [f32; 4]) -> f32 {
    let speed = if speed <= 0.0 { 0.0 - speed } else { speed };
    let [x, y, ..] = vector;
    let z = 0.0_f32;
    let length = ((x * x + y * y) + z * z).sqrt();
    if 0.01 < length {
        let scale = 1.0 / length;
        let (x, y) = (scale * x, scale * y);
        if y < 0.0 {
            let mut across = x / sin_60();
            if across <= 0.0 {
                across = 0.0 - across;
            }
            let across = if 0.0 <= across { across.min(1.0) } else { 0.0 };
            return (1.0 - across) * speed;
        }
    }
    0.0
}

/// `E16Filter::Step` (0x48af90) for one channel: the target `a`, the previous output `b`, the step `dt`, and the speed
/// fade input. The asm's order of operations is kept: the curve is `((c3 * m2 + c2) * m2 + lin * m) * sign` with
/// `m2 = m * m`, the fade scales the target and clamps it to [-1, 1], then the rise and fall limits act on the
/// magnitude (the sign is taken off first and put back last).
pub fn step(cell: &Cell, a: f32, b: f32, dt: f32, speed: f32) -> f32 {
    if !cell.active || a == b {
        return a;
    }
    let sign = if a >= 0.0 { 1.0 } else { -1.0 };
    let m = if a > 0.0 { a } else { 0.0 - a };
    let mut a = if cell.nonlinear {
        let m2 = m * m;
        ((cell.c3 * m2 + cell.c2) * m2 + cell.lin * m) * sign
    } else {
        a
    };
    let mut scale = 1.0;
    if cell.fade != 0.0 && cell.fade_speed != 0.0 {
        scale = speed_scale(speed, cell.fade, cell.fade_speed);
        a = (a * scale).clamp(-1.0, 1.0);
    }
    if cell.rise == 0.0 && cell.fall == 0.0 {
        return a;
    }
    let (mut a, b) = if sign < 0.0 { (0.0 - a, 0.0 - b) } else { (a, b) };
    if b <= a {
        if b >= 0.0 {
            if b < a && cell.rise != 0.0 {
                let up = cell.rise * scale * dt + b;
                if up < a {
                    a = up;
                }
            }
        } else if cell.fall != 0.0 {
            a = cell.fall * scale * dt + b;
            if a >= 0.0 {
                a = 0.0;
            }
        }
    } else if cell.fall != 0.0 {
        let down = b - cell.fall * scale * dt;
        if a < down {
            a = down;
        }
    }
    if sign > 0.0 {
        a
    } else {
        0.0 - a
    }
}

/// `E16Filter::StepB` (0x48b1d0) with its `f` argument 0, the only call `EvalA` (0x48bbe0) makes. The other branch
/// reads `NotifyA`'s vectors and is not ported. Written from the decompile, not checked against the asm: nothing on a
/// drive's path calls it.
pub fn step_b(a: f32, b: f32, c: f32, d: f32, e: f32) -> f32 {
    let diff = a - b;
    let s = if diff <= 0.0 {
        if diff >= 0.0 {
            0.0
        } else {
            -1.0
        }
    } else {
        1.0
    };
    let c = if b.abs() > a.abs() { d } else { c };
    let b = s * c * e + b;
    if b >= -1.0 {
        if b <= 1.0 {
            b
        } else {
            1.0
        }
    } else {
        -1.0
    }
}

/// The raw control as `E16Filter::NotifyB` (0x48ab40) takes it from the poll (`CSigCarControlInput`), by the offsets it
/// reads. Fields the filter's output does not use (+0x40, +0x44) are left out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Raw {
    /// +0x00: action bits; the filter ORs them into its accumulator until the next tic.
    pub flags: u32,
    /// +0x04 and +0x08, stored as they come.
    pub aux: u32,
    pub byte2: u32,
    /// +0x14 to +0x24: steering, then channels 1 to 4.
    pub channels: [f32; CHANNELS],
    /// +0x28, the low byte.
    pub gear: u8,
    /// +0x2c to +0x3c: the cell selectors.
    pub select: [u32; CHANNELS],
}

/// `CSigCarControlFiltered`, the filter's output at `E16Filter` +0x310 (what `Tic` emits and `Pack` reads).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Filtered {
    /// +0x00: the action bits accumulated since the previous tic.
    pub flags: u32,
    /// +0x04 and +0x08.
    pub aux: u32,
    pub byte2: u32,
    /// +0x14 to +0x24.
    pub channels: [f32; CHANNELS],
    /// +0x28.
    pub gear: u8,
}

/// The filter's state: the grid, the flags accumulated since the last tic, the last poll, the last outputs, and the
/// speed fade input.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    grid: Grid,
    flags: u32,
    input: Raw,
    prev: [f32; CHANNELS],
    speed: f32,
}

impl Filter {
    pub fn new(grid: Grid) -> Self {
        Self { grid, flags: 0, input: Raw::default(), prev: [0.0; CHANNELS], speed: 0.0 }
    }

    /// `E16Filter::NotifyB` (0x48ab40): stores one poll's control.
    pub fn notify(&mut self, raw: &Raw) {
        self.flags |= raw.flags;
        self.input = Raw { flags: 0, ..*raw };
        self.grid.select = raw.select;
    }

    /// The speed fade input, `m_36c` ([`notify_a`]).
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    /// `E16Filter::Tic` (0x48c500): filters every channel once from its raw value and its last output, and clears the
    /// accumulated flags. `dt` is the physics step (`MaxTimeStep`).
    pub fn tic(&mut self, dt: f32) -> Filtered {
        let channels: [f32; CHANNELS] =
            std::array::from_fn(|i| step(self.grid.cell(i), self.input.channels[i], self.prev[i], dt, self.speed));
        let out = Filtered {
            flags: self.flags,
            aux: self.input.aux,
            byte2: self.input.byte2,
            channels,
            gear: self.input.gear,
        };
        self.prev = channels;
        self.flags = 0;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_physics::math::trig::x87_sin;

    #[test]
    fn sin_60_is_the_x87_result_rounded() {
        assert_eq!(std::f32::consts::FRAC_PI_3.to_bits(), 0x3f86_0a92, "the asm's 1.0471976f");
        assert_eq!(sin_60(), x87_sin(std::f32::consts::FRAC_PI_3) as f32);
    }

    #[test]
    fn notify_a_fades_the_speed_by_the_sideways_share() {
        // Straight ahead (-y): no sideways share, the whole speed.
        assert_eq!(notify_a(20.0, [0.0, -5.0, 0.0, 0.0]), 20.0);
        // Sideways at 60 degrees or more: nothing is left.
        assert_eq!(notify_a(20.0, [sin_60() * 5.0, -2.5, 7.0, 0.0]), 0.0);
        // The lane 2 is ignored; x's sign does not matter.
        let a = notify_a(20.0, [1.0, -4.0, 9.0, 0.0]);
        assert_eq!(a, notify_a(20.0, [-1.0, -4.0, -9.0, 0.0]));
        assert!(a < 20.0 && a > 0.0);
        // The speed's sign is dropped.
        assert_eq!(notify_a(-20.0, [0.0, -5.0, 0.0, 0.0]), 20.0);
    }

    #[test]
    fn notify_a_is_zero_when_the_vector_does_not_point_back_or_is_short() {
        assert_eq!(notify_a(20.0, [0.0, 5.0, 0.0, 0.0]), 0.0);
        assert_eq!(notify_a(20.0, [0.0, 0.0, 0.0, 0.0]), 0.0);
        assert_eq!(notify_a(20.0, [0.005, -0.005, 0.0, 0.0]), 0.0);
        assert_eq!(notify_a(20.0, [f32::NAN, -5.0, 0.0, 0.0]), 0.0);
    }

    fn cell() -> Cell {
        Cell { active: true, ..Cell::INITIAL }
    }

    #[test]
    fn inactive_or_unchanged_passes_the_raw_value() {
        let c = Cell { active: false, rise: 3.0, ..Cell::INITIAL };
        assert_eq!(step(&c, 0.4, 0.0, 0.007, 0.0), 0.4);
        assert_eq!(step(&cell(), 0.4, 0.4, 0.007, 0.0), 0.4);
    }

    #[test]
    fn linear_cell_without_rates_or_fade_is_the_identity() {
        assert_eq!(step(&cell(), -0.75, 0.0, 0.007, 30.0), -0.75);
    }

    #[test]
    fn rise_limit_moves_at_the_rate_then_reaches_the_target() {
        let c = Cell { rise: 3.0, ..cell() };
        let dt = 0.007;
        let first = step(&c, 1.0, 0.0, dt, 0.0);
        assert_eq!(first.to_bits(), (3.0f32 * 1.0 * dt + 0.0).to_bits());
        assert_eq!(step(&c, 1.0, 0.99, dt, 0.0), 1.0);
    }

    #[test]
    fn fall_limit_steps_down_from_the_previous_output() {
        let c = Cell { fall: 5.0, ..cell() };
        // target 0 below the previous output 1: the output falls by fall * dt per step
        let out = step(&c, 0.0, 1.0, 0.007, 0.0);
        assert_eq!(out.to_bits(), (1.0f32 - 5.0f32 * 1.0 * 0.007).to_bits());
        assert!(step(&c, 0.0, out, 0.007, 0.0) < out);
    }

    #[test]
    fn fade_scales_and_clamps_the_target() {
        let c = Cell { fade: 0.5, fade_speed: 27.0, ..cell() };
        assert_eq!(speed_scale(0.0, 0.5, 27.0), 1.0 - 0.0 * 0.5);
        assert_eq!(speed_scale(54.0, 0.5, 27.0), 0.5);
        assert_eq!(step(&c, 1.0, 0.0, 0.007, 54.0), 0.5);
    }

    #[test]
    fn reset_recomputes_the_flags_from_the_values() {
        let mut grid = Grid { cells: [[Cell::INITIAL; CELLS]; CHANNELS], select: [0; CHANNELS] };
        grid.reset();
        assert!(grid.cells.iter().flatten().all(|c| !c.active && !c.nonlinear));
        grid.cells[0][0].fade = 0.3;
        grid.select = [3; CHANNELS];
        grid.reset();
        assert!(grid.cells[0][0].active);
        assert_eq!(grid.select, [0; CHANNELS]);
    }

    #[test]
    fn settings_load_the_steering_cells_and_reset_their_flags() {
        let ini = Ini::parse(
            "[STEER::BUTTON_DIGITAL]\nActive = true\nRiseRate = 3.0\nFallRate = 5.0\nmaxSpeedFade = 0.5\n\
             MaxSpeedFadeSpeed = 27.0\n[STEER::AXIS]\nActive = false\nParabolic = false\nmaxSpeedFade = 0.3\n\
             MaxSpeedFadeSpeed = 35.0\n",
        );
        let grid = Grid::from_settings(&ini);
        // AXIS: "Active = false" is overwritten by the reset: its fade is set
        let axis = grid.cells[0][0];
        assert!(axis.active && !axis.nonlinear);
        assert_eq!((axis.fade, axis.fade_speed), (0.3, 35.0));
        let button = grid.cells[0][3];
        assert_eq!((button.rise, button.fall), (3.0, 5.0));
        assert_eq!(grid.cells[0][2], Cell::INITIAL);
    }

    #[test]
    fn step_b_clamps_toward_the_target_at_the_step_size() {
        assert_eq!(step_b(0.5, 0.0, 0.1, 0.9, 0.0), 0.0);
        assert_eq!(step_b(0.5, 0.0, 0.1, 0.9, 1.0), 0.1);
        assert_eq!(step_b(0.5, 0.9, 0.1, 0.9, 5.0), -1.0);
    }

    #[test]
    fn tic_copies_the_accumulated_flags_and_clears_them() {
        let mut f = Filter::new(Grid::from_settings(&Ini::default()));
        f.notify(&Raw { flags: 1, ..Raw::default() });
        f.notify(&Raw { flags: 4, aux: 1, ..Raw::default() });
        let out = f.tic(0.007);
        assert_eq!((out.flags, out.aux), (5, 1));
        assert_eq!(f.tic(0.007).flags, 0);
    }
}
