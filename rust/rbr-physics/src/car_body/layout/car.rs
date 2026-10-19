//! `CCar`'s own words: the `CReal` record, the member offsets, the getters and setters the car-body ports read and
//! write, the flow and timer words.

use super::car_parts::{wheel_list_at, TimeKeeperDiff};
use super::construct::{DEF_DANGEROUS_ACCELERATION, DEF_DRAG_COEFFICIENT, DEF_FRONTAL_AREA};
use crate::car_body::e36::{self, Triple};
use crate::car_body::links::{Link, LinkRef, Links, List, ListRef};
use crate::car_body::store::CarStore;
use crate::math::quat::Quat;
use crate::math::vec::{Vec3, Vec4};
use crate::math::Mat3;
use crate::mem::Mem;
pub use crate::physics_core::layout::PHYSICS_GLOBAL as WORLD;
use crate::store::Raw;

/// A `CReal` record (0x10 bytes): the dirty flag at +0x8 (`CBase::SetDirty` writes 1) and the value at +0xc.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Real {
    pub dirty: u32,
    pub value: f32,
}

impl Real {
    pub fn load<R: Raw + ?Sized>(mem: &R, at: u32) -> Self {
        Real { dirty: mem.u32(at + 0x08), value: mem.f32(at + 0x0c) }
    }

    pub fn store<R: Raw + ?Sized>(&self, mem: &mut R, at: u32) {
        mem.set_u32(at + 0x08, self.dirty);
        mem.set_f32(at + 0x0c, self.value);
    }
}

/// Offset of the state object (`CCar::CState`, 0xc70 bytes) in the car: `CCar::StateAtb` returns it.
pub const STATE: u32 = 0xa00;

/// The car's state object address, as `CCar::StateAtb` returns it.
pub fn state_address(car: u32) -> u32 {
    car + STATE
}

/// `CCar` members the car-body ports read and write (`notes/physics/car-body.md` 2a).
pub const STEP_COUNT: u32 = 0x950;
pub const SNAPSHOT_ON: u32 = 0x9e4;
pub const PARTIAL_CONTACT: u32 = 0x9e8;
pub const WALL_CONTACT: u32 = 0x9ec;
pub const DAMAGE_HANDLER: u32 = 0x9f4;
pub const ANGULAR_VELOCITY: u32 = 0x190;
pub const VELOCITY: u32 = 0x1c0;
pub const RACK_RATE: u32 = 0xb1c;
pub const SPEED: u32 = 0x15fc;
pub const SPEED_PRE: u32 = 0x1600;
pub const DIR_BACK: u32 = 0x1610;
pub const FLAG_50G: u32 = 0x1668;
pub const WALL_PENETRATION: u32 = 0x166c;
pub const T_OVERRIDE: u32 = 0x1670;
pub const STEER_INPUT: u32 = 0x16a4;
pub const GRID_PUSHED: u32 = 0x16a8;

/// The embedded `E47SolidScratch` member used by `CCar::Vf54x`.
pub fn scratch1690(car: u32) -> u32 {
    car + 0x1690
}

/// The global `E60Flow::M5666b0` answers with when the flow's host object is not uniform.
const DAMAGE_SELECT: u32 = 0x0166_084C;

/// The global whose +0x8e84 word is the flow's mode when the host object is not tagged 0x7b (`E13`'s pointer).
const HOST_GLOBAL: u32 = 0x007D_6128;

/// Where `matrix_cell(i, j)` lives.
pub fn cell_at(car: u32, i: u32, j: u32) -> u32 {
    car + 0xa1c + (j + 4 * i) * 0x10
}

/// The CReal of the steering slew (+0xaf0; its value at +0xafc, `m_steerSlew`), which `StepEngine` moves.
pub fn steer_slew_at(car: u32) -> u32 {
    car + 0xaf0
}

/// The CReal of the steering target (+0xb00; its value is the word at +0xb0c, `target_b0c`).
pub fn target_b0c_at(car: u32) -> u32 {
    car + 0xb00
}

/// The CReal of the rate of `target_b0c` (+0xb10; its value is the word at +0xb1c, `rate_b1c`).
pub fn rate_b1c_at(car: u32) -> u32 {
    car + 0xb10
}

/// The CReal of the steering torque sum (+0xad0; its value at +0xadc, see `steer_torque_sum`).
pub fn steer_torque_sum_at(car: u32) -> u32 {
    car + 0xad0
}

/// The address of damper `i`'s CReal (`CCar::Damp66D0`): the cell at +0xa28 + 0x40 i, its CReal at +0x14.
pub fn damper_real_at(car: u32, i: u32) -> u32 {
    car + 0xa28 + 0x40 * i + 0x14
}

/// `CCarLightSystem::Attach(flags)` (0x4f1e00) on the light system at `lights`: the flag block (`+0x14`) and its words
/// +0, +0xc, +0x14, +8, +4, +0x18 and +0x10 set to 1, in the asm's order.
pub fn attach_lights<S: CarStore>(mem: &mut S, lights: u32, flags: u32) {
    mem.set_u32(lights + 0x14, flags);
    for off in [0, 0xc, 0x14, 0x8, 0x4, 0x18, 0x10] {
        mem.set_u32(flags + off, 1);
    }
}

/// The link words: next at +0, prev at +4, owner at +8 (`CBodyLink`); zero is no pointer.
fn optional(word: u32) -> Option<u32> {
    (word != 0).then_some(word)
}

impl Links for Mem {
    fn link(&self, at: LinkRef) -> Link {
        let at = at.addr();
        Link {
            next: optional(self.u32(at)).map(LinkRef::new),
            prev: optional(self.u32(at + 4)).map(LinkRef::new),
            owner: optional(self.u32(at + 8)).map(ListRef::new),
        }
    }

    fn set_link(&mut self, at: LinkRef, link: Link) {
        self.set_next(at, link.next);
        self.set_prev(at, link.prev);
        self.set_u32(at.addr() + 8, link.owner.map_or(0, ListRef::addr));
    }

    fn set_next(&mut self, at: LinkRef, next: Option<LinkRef>) {
        self.set_u32(at.addr(), next.map_or(0, LinkRef::addr));
    }

    fn set_prev(&mut self, at: LinkRef, prev: Option<LinkRef>) {
        self.set_u32(at.addr() + 4, prev.map_or(0, LinkRef::addr));
    }

    /// The list head words: first at +0, last at +4, count at +8 (`E34LinkList10`).
    fn list(&self, at: ListRef) -> List {
        let at = at.addr();
        List {
            first: optional(self.u32(at)).map(LinkRef::new),
            last: optional(self.u32(at + 4)).map(LinkRef::new),
            count: self.u32(at + 8),
        }
    }

    fn set_list(&mut self, at: ListRef, list: List) {
        let at = at.addr();
        self.set_u32(at, list.first.map_or(0, LinkRef::addr));
        self.set_u32(at + 4, list.last.map_or(0, LinkRef::addr));
        self.set_u32(at + 8, list.count);
    }
}

/// The Mem store: the bodies are the layout's reads and writes.
impl CarStore for Mem {
    /// Reads the matrix of `at` (nine words, row by row).
    fn load_mat3(&self, at: u32) -> Mat3 {
        let mem = self;
        std::array::from_fn(|r| std::array::from_fn(|c| mem.f32(at + 4 * (3 * r + c) as u32)))
    }

    /// `CCar::Get30` (0x4e67f0): the word at `this + 0x30`.
    fn get_30(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x30)
    }

    /// `CCar::Get15F8` (0x4e6770, thiscall, st0): the float at `this + 0x15f8`, the car speed the slip query reads.
    fn get_15f8(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + 0x15F8)
    }

    /// `E60Flow::M5666b0` (0x5666b0, fastcall, `flow` in ecx): the host object's word at `+0x17854` when its first word is
    /// 0x7b (the `short` compare), else the global `DAMAGE_SELECT`. `MotionProbe` asks it before its probe.
    fn damage_select(&self, flow: u32) -> u32 {
        let mem = self;
        let host = mem.u32(flow + 0xC98);
        if mem.u8(host) == 0x7B && mem.u8(host + 1) == 0 {
            mem.u32(host + 0x17854)
        } else {
            mem.u32(DAMAGE_SELECT)
        }
    }

    /// `E60Flow::M566640` (0x566640, thiscall, `flow` in ecx): 1 when the flow holds, else 0. The state word is the host's
    /// +0x17850 when the host's first word is 0x7b (the `short` compare), else the flow's own first word. The flow holds
    /// unless that word is 3 and the mode word (the host's +0x17858, else the global's +0x8e84) is 2 or more.
    fn flow_holds(&self, flow: u32) -> u32 {
        let mem = self;
        let host = mem.u32(flow + 0xC98);
        let tagged = mem.u8(host) == 0x7B && mem.u8(host + 1) == 0;
        let state = if tagged { mem.u32(host + 0x17850) } else { mem.u32(flow) };
        if state != 3 {
            return 1;
        }
        let mode = if tagged { mem.u32(host + 0x17858) } else { mem.u32(mem.u32(HOST_GLOBAL) + 0x8E84) };
        u32::from(mode < 2)
    }

    /// The lane word at `+0xa48 + 0x40 * i` (`CCar::GetA48`). Meaning unknown; the drive calls it with `i` 0 and 1.
    fn lane_a48(&self, car: u32, i: u32) -> f32 {
        let mem = self;
        mem.f32(car + 0xa48 + 0x40 * i)
    }

    /// The CReal at +0xad0: its value (+0xadc) is the steered wheels' torque sum (`AddADC`; `notes/ngp-ffb.md`).
    fn steer_torque_sum(&self, car: u32) -> Real {
        let mem = self;
        Real::load(mem, car + 0xad0)
    }

    /// The CReal at `+0xa1c + 0x10 * (j + 4 * i)` (`CCar::SetA1C`). Meaning unknown.
    fn matrix_cell(&self, car: u32, i: u32, j: u32) -> Real {
        let mem = self;
        Real::load(mem, cell_at(car, i, j))
    }

    /// The word at +0xb0c (`CCar::GetB0C`): the steering rack position (`notes/ngp-ffb.md`).
    fn target_b0c(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + 0xb0c)
    }

    /// The word at +0xb1c (`CCar::GetB1C`): the rate of `target_b0c`, by `car-body.md`.
    fn rate_b1c(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + RACK_RATE)
    }

    /// The target the slew moves toward (`m_steerInput`, +0x16a4).
    fn steer_input(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + STEER_INPUT)
    }

    /// The setup word at `def + 0x22c`, `MaxSteeringLock` (the value of the key's CReal at `+0x220`).
    fn max_steering_lock(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + 0x22c)
    }

    /// The snapshot flag (+0x9e4, `m_snapshotOn`): `StepEngine` calls `Snap9360` while it is set.
    fn snapshot_on(&self, car: u32) -> bool {
        let mem = self;
        mem.u32(car + SNAPSHOT_ON) != 0
    }

    /// The override timer (+0x1670): while above 0 the filtered steering torque is held at 0 (`CCar::UpdateFfbTorque`).
    fn t_override(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + T_OVERRIDE)
    }

    /// The car speed (+0x15fc), m/s (`car-body.md` 2a, inferred).
    fn speed(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + SPEED)
    }

    /// The filtered steering torque, which force feedback reads: the CReal at +0xae0 (its value at +0xaec).
    fn ffb_torque(&self, car: u32) -> Real {
        let mem = self;
        Real::load(mem, car + 0xae0)
    }

    /// The car's setup (`CCar::CDef`) address: the pointer at +0x34.
    fn def_address(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x34)
    }

    /// The setup word at `def + 0x23c`, `SteeringRackRatio` (the value of the key's CReal at `+0x230`).
    fn steering_rack_ratio(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + 0x23c)
    }

    /// The drag coefficient `Cd` (`def + 0x4490`, `car-body.md`: `DragCoefficient`).
    fn drag_coefficient(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + DEF_DRAG_COEFFICIENT)
    }

    /// The frontal area `A` (`def + 0x4480`, `car-body.md`: `FrontArea`).
    fn frontal_area(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + DEF_FRONTAL_AREA)
    }

    /// The car's velocity (`CCar::m_velocity`, three lanes at +0x1c0).
    fn velocity(&self, car: u32) -> Vec3 {
        let mem = self;
        Vec3::load(mem, car + VELOCITY)
    }

    /// The car's angular velocity (`CCar::m_angularVelocity`, three lanes at +0x190).
    fn angular_velocity(&self, car: u32) -> Vec3 {
        let mem = self;
        Vec3::load(mem, car + ANGULAR_VELOCITY)
    }

    /// The first and fifth lanes of damper `i` (at +0xa28 + 0x40 i and +0x10 more): their difference is its force.
    fn damper_lanes(&self, car: u32, i: u32) -> (f32, f32) {
        let mem = self;
        let at = car + 0xa28 + 0x40 * i;
        (mem.f32(at), mem.f32(at + 0x10))
    }

    /// The setup gain of damper `i` (`def + 0x4460 + 0x10 i`).
    fn damper_gain(&self, car: u32, i: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + 0x4460 + 0x10 * i)
    }

    /// The three inertia-like words of the def (`def + 0x130 + 4 i`, `CCar::Vf64x`'s divisors; their meaning is open).
    fn def_inertia(&self, car: u32) -> Vec3 {
        let mem = self;
        Vec3::load(mem, mem.def_address(car) + 0x130)
    }

    /// The vector at +0x15d0 (`CCar::Vf64x` blends it; its meaning is open). All four lanes are modelled: the blend runs
    /// `mulps` and `addps` over four lanes, and lane 3 comes from the walk's copy of its input's lane 3.
    fn aux_vector(&self, car: u32) -> Vec4 {
        let mem = self;
        let [x, y, z, w] = mem.f32s(car + 0x15d0);
        Vec4 { x, y, z, w }
    }

    /// Stores the four lanes of the vector at +0x15d0 (`aux_vector`).
    fn set_aux_vector(&mut self, car: u32, v: Vec4) {
        let mem = self;
        for (k, x) in [v.x, v.y, v.z, v.w].into_iter().enumerate() {
            mem.set_f32(car + 0x15d0 + 4 * k as u32, x);
        }
    }

    /// The scalar at +0x15d4 (`CCar::Vf64x`, the second argument of its external 0x495090).
    fn aux_scalar(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + 0x15d4)
    }

    /// The word at +0x16ac (`CCar::Vf64x`): zero or not decides the external call.
    fn aux_flag(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x16ac)
    }

    /// The keeper's word at +0x160e0 (`CCar::Vf64x` tests bit 0x10 of it).
    fn keeper_flags(&self) -> u32 {
        let mem = self;
        let world = mem.u32(WORLD);
        let keeper = mem.u32(world + 0x24);
        mem.u32(keeper + 0x160e0)
    }

    /// The object pointer at +0x490 + 4 i (`CCar::Vf64x`'s loop over four objects).
    fn car_object(&self, car: u32, i: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x490 + 4 * i)
    }

    /// The brake-wall penetration (`m_wallPenetration`, +0x166c, m): `Sim7AD0` acts only while it is above 0.
    fn wall_penetration(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + WALL_PENETRATION)
    }

    /// The vector at +0x370, the seed `E46_Get370` returns (`Sim7AD0`'s `ToWorld` input). S70 adds into it.
    fn accumulator(&self, car: u32) -> Vec3 {
        let mem = self;
        Vec3::load(mem, car + 0x370)
    }

    /// The inverse mass (`def + 0x16c`, `car-body.md`): `Sim7AD0` divides by it.
    fn inverse_mass(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + 0x16c)
    }

    /// The wind (`Sim7AD0`'s gust): the world keeper's vector at `+0x160a0 + 0x10`.
    fn wind(&self) -> Vec3 {
        let mem = self;
        let world = mem.u32(WORLD);
        let keeper = mem.u32(world + 0x24);
        Vec3::load(mem, keeper + 0x160b0)
    }

    /// The time keeper's gate (`CCar::DragAndSpinDamp`): bit 0x4000 of the keeper's word at +0x160f4 or of the world's
    /// word at +0x38. The keeper is the world's pointer at +0x24.
    fn time_keeper_gate(&self) -> bool {
        let mem = self;
        let world = mem.u32(WORLD);
        let keeper = mem.u32(world + 0x24);
        (mem.u32(keeper + 0x160f4) | mem.u32(world + 0x38)) & 0x4000 != 0
    }

    /// The body the car's transform reads (`+0x4a0`, `CCar::Xform6870`). The asm checks it for null first (the assert path).
    fn body(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x4a0)
    }

    /// Stores the body (`+0x4a0`): `Vf150x` makes a wheel the body when its flags say so.
    fn set_body(&mut self, car: u32, wheel: u32) {
        let mem = self;
        mem.set_u32(car + 0x4a0, wheel);
    }

    /// `E19IsState2` (0x499900, fastcall on the replay object at the global 0x893634): 1 when its state word (+0x10) is 2,
    /// else 0.
    fn replay_is_state2(&self, replay: u32) -> u32 {
        let mem = self;
        u32::from(mem.u32(replay + 0x10) == 2)
    }

    /// The world's timer card (`*(world + 0x20)`, the timers' owner).
    fn timer_card(&self) -> u32 {
        let mem = self;
        mem.u32(mem.u32(WORLD) + 0x20)
    }

    /// The timer's double (`*(timer card + 0x20)`), which the timekeeper records and the drive line's thunk reads.
    fn timer_double(&self) -> f64 {
        let mem = self;
        let at = mem.timer_card() + 0x20;
        f64::from_bits(u64::from(mem.u32(at)) | u64::from(mem.u32(at + 4)) << 32)
    }

    /// The timer card's word at +0x18 (the position payload's last word).
    fn timer_word_18(&self) -> u32 {
        let mem = self;
        mem.u32(mem.timer_card() + 0x18)
    }

    /// The car's position lanes (`car + 0x140`, four words), the start of the position payload.
    fn position_words(&self, car: u32) -> [u32; 4] {
        let mem = self;
        std::array::from_fn(|k| mem.u32(car + 0x140 + 4 * k as u32))
    }

    /// The payload's owner word (`car + 0x30`).
    fn owner_word(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x30)
    }

    /// The float at `*(car + 0x4b8) + 0x45c` (the MrR payload's last word, as bits).
    fn mrr_word(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(mem.u32(car + 0x4B8) + 0x45C)
    }

    /// The E39 block (+0x620): its count word (+0x80) and scale (+0x8c) are the sensor payload's words 11 and 12.
    fn sensor_block(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x620)
    }

    /// The count word of the E39 block (`+0x80`, read as a signed integer by `Sample9800`).
    fn sensor_count(&self, block: u32) -> u32 {
        let mem = self;
        mem.u32(block + 0x80)
    }

    /// The scale word of the E39 block (`+0x8c`, a float).
    fn sensor_scale(&self, block: u32) -> f32 {
        let mem = self;
        mem.f32(block + 0x8c)
    }

    /// The timer gap's arguments (`0x4a0290`): the keeper, which is the world's word at +0x40 (`ecx`), and the address of
    /// the timer's double, the card's +0x20 plus 0x20 (pushed).
    fn timer_gap_args(&self) -> (u32, u32) {
        let mem = self;
        let world = mem.u32(WORLD);
        (mem.u32(world + 0x40), mem.u32(world + 0x20) + 0x20)
    }

    fn time_keeper_diff(&self, keeper: u32, current: u32) -> TimeKeeperDiff {
        let mem = self;
        let card = mem.u32(keeper + 4);
        let end = if mem.u32(card + 0x58) != 0 { mem.load_f64(card + 0x50) } else { mem.load_f64(current) };
        TimeKeeperDiff { origin: mem.load_f64(card + 0x38), end }
    }

    fn load_f64(&self, at: u32) -> f64 {
        let mem = self;
        f64::from_bits(u64::from(mem.u32(at)) | u64::from(mem.u32(at + 4)) << 32)
    }

    /// The word at +0x650 of a wheel's object, dereferenced `k` words: `*(wheel + 0x650) + 4 k` (`Vf6cx`'s tests).
    fn wheel_flag(&self, wheel: u32, k: u32) -> u32 {
        let mem = self;
        mem.u32(mem.u32(wheel + 0x650) + 4 * k)
    }

    /// The wheel at +0x470 + 4 i (`CCar::m_wheels`), or None when the slot is empty (`GetDir` reads wheels 0 and 1).
    fn wheel(&self, car: u32, i: u32) -> Option<u32> {
        let mem = self;
        let wheel = mem.u32(car + 0x470 + 4 * i);
        (wheel != 0).then_some(wheel)
    }

    /// The wheel's attach flags (+0x60): the bits `Vf150x` tests.
    fn attach_flags(&self, wheel: u32) -> u32 {
        let mem = self;
        mem.u32(wheel + 0x60)
    }

    /// The wheel's def (+0x34, a `CDef`); its slot indices are at +0x4fc and +0x23c.
    fn wheel_def(&self, wheel: u32) -> u32 {
        let mem = self;
        mem.u32(wheel + 0x34)
    }

    /// Stores the wheel's index in the car's wheel list (+0xeb4).
    fn set_wheel_index(&mut self, wheel: u32, i: u32) {
        let mem = self;
        mem.set_u32(wheel + 0xEB4, i);
    }

    /// The direction input of a wheel: the vector at +0x9e0 (lanes 0 to 2; `GetDir` sums them).
    fn wheel_direction(&self, wheel: u32) -> Vec3 {
        let mem = self;
        Vec3 { x: mem.f32(wheel + 0x9E0), y: mem.f32(wheel + 0x9E4), z: mem.f32(wheel + 0x9E8) }
    }

    /// The grid object the car pushes its edge to (`CCar` +0x64, an `E37GridOps`); zero when there is none.
    fn grid_object(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x64)
    }

    /// The grid flag `m_gridPushed` (+0x16a8): nonzero once the car's edge is in the grid (`PushGridEdge` sets it).
    fn grid_pushed(&self, car: u32) -> bool {
        let mem = self;
        mem.u32(car + GRID_PUSHED) != 0
    }

    /// Stores the grid flag (`layout::grid_pushed`), 1 when set.
    fn set_grid_pushed(&mut self, car: u32, pushed: bool) {
        let mem = self;
        mem.set_u32(car + GRID_PUSHED, pushed as u32);
    }

    /// The `+0xebc` word of the wheel at +0x470 + 4 i (`Vf6cx` counts the wheels that are on).
    fn wheel_ebc(&self, car: u32, i: u32) -> u32 {
        let mem = self;
        mem.u32(mem.u32(car + 0x470 + 4 * i) + 0xEBC)
    }

    /// Stores the brake-wall penetration (+0x166c, `m_wallPenetration`).
    fn set_wall_penetration(&mut self, car: u32, v: f32) {
        let mem = self;
        mem.set_f32(car + WALL_PENETRATION, v);
    }

    /// The time the wall has been in contact (+0x9ec, `m_tWallContact`).
    fn wall_contact(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + WALL_CONTACT)
    }

    /// Stores the time the wall has been in contact (+0x9ec).
    fn set_wall_contact(&mut self, car: u32, v: f32) {
        let mem = self;
        mem.set_f32(car + WALL_CONTACT, v);
    }

    /// The partial-contact time (+0x9e8, `m_tPartialContact`): `Vf6cx` adds the step to it.
    fn partial_contact(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + PARTIAL_CONTACT)
    }

    /// Stores the partial-contact time (+0x9e8).
    fn set_partial_contact(&mut self, car: u32, v: f32) {
        let mem = self;
        mem.set_f32(car + PARTIAL_CONTACT, v);
    }

    /// Stores the override timer (+0x1670, `m_tOverride`).
    fn set_t_override(&mut self, car: u32, v: f32) {
        let mem = self;
        mem.set_f32(car + T_OVERRIDE, v);
    }

    /// Sets the 50g flag (+0x1668, `m_flag50g`) to 1.
    fn set_flag_50g(&mut self, car: u32) {
        let mem = self;
        mem.set_u32(car + FLAG_50G, 1);
    }

    /// Stores the speed before the update (+0x1600, `m_speedPre`).
    fn set_speed_pre(&mut self, car: u32, v: f32) {
        let mem = self;
        mem.set_f32(car + SPEED_PRE, v);
    }

    /// Stores the speed (+0x15fc, `m_speed`).
    fn set_speed(&mut self, car: u32, v: f32) {
        let mem = self;
        mem.set_f32(car + SPEED, v);
    }

    /// Stores the negated y of the walk of the velocity (+0x15f8; its meaning is open).
    fn set_neg_walk_y(&mut self, car: u32, v: f32) {
        let mem = self;
        mem.set_f32(car + 0x15F8, v);
    }

    /// The direction opposite the velocity, four lanes (+0x1610, `m_dirBack`).
    fn set_dir_back(&mut self, car: u32, v: Vec4) {
        let mem = self;
        for (k, x) in [v.x, v.y, v.z, v.w].into_iter().enumerate() {
            mem.set_f32(car + DIR_BACK + 4 * k as u32, x);
        }
    }

    /// The velocity with its fourth lane (+0x1c0, four lanes; `velocity` is the first three).
    fn velocity4(&self, car: u32) -> Vec4 {
        let mem = self;
        let [x, y, z, w] = mem.f32s(car + VELOCITY);
        Vec4 { x, y, z, w }
    }

    /// The three words of the def at +0x230, +0x234, +0x238 (`Vf6cx`'s 50g test squares them).
    fn accel_lanes(&self, car: u32) -> Vec3 {
        let mem = self;
        Vec3::load(mem, car + 0x230)
    }

    /// The dangerous acceleration (`CDef::m_dangerousAcceleration`, def +0x44b0).
    fn dangerous_acceleration(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + DEF_DANGEROUS_ACCELERATION)
    }

    /// The setup word at `def + 0x44a0`, `MaxFFTorque` (the value of the key's CReal at `+0x4494`), which `FfbNorm`
    /// divides by.
    fn max_ff_torque(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + 0x44A0)
    }

    /// The setup word at `def + 0x24c` (a working name): the gain of the drive's torque term in `Vf6cx`'s position.
    fn def_gain_24c(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(mem.def_address(car) + 0x24C)
    }

    /// The word at +0x15f4 (the position's running value, `Vf6cx` and the payload's `* 0.001`).
    fn value_15f4(&self, car: u32) -> f32 {
        let mem = self;
        mem.f32(car + 0x15F4)
    }

    /// The car's four wheel pointers (+0x470 to +0x47c), read as they are (`CCar::UpdateA720` calls each one).
    fn wheel_pointers(&self, car: u32) -> [u32; 4] {
        let mem = self;
        std::array::from_fn(|i| mem.u32(wheel_list_at(car, i as u32)))
    }

    /// The object the seed's frame words land in (the car's +0x34).
    fn frame_object(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x34)
    }

    /// The body gate of the point scatters (`CCarBody::Vf9Cx` and `VfA0x`): bit 0x1000 of the keeper's word at +0x160f4 or
    /// of the world's word at +0x38. The keeper is the world's pointer at +0x24, as in `time_keeper_gate`.
    fn body_gate(&self) -> bool {
        let mem = self;
        let world = mem.u32(WORLD);
        let keeper = mem.u32(world + 0x24);
        (mem.u32(keeper + 0x160f4) | mem.u32(world + 0x38)) & 0x1000 != 0
    }

    /// The base the scatters add or subtract: the lanes at +0x130 of the object at `body + 0x34`. None when that object's
    /// word at +0x120 is null: the original faults then.
    fn body_base(&self, body: u32) -> Option<Vec3> {
        let mem = self;
        let owner = mem.u32(body + 0x34);
        (mem.u32(owner + 0x120) != 0).then(|| Vec3 {
            x: mem.f32(owner + 0x130),
            y: mem.f32(owner + 0x134),
            z: mem.f32(owner + 0x138),
        })
    }

    /// The `n` points at `out` (sixteen bytes each, lanes 0 to 3).
    fn load_points(&self, out: u32, n: u32) -> Vec<Vec4> {
        let mem = self;
        (0..n).map(|i| Vec4::load(mem, out + 16 * i)).collect()
    }

    /// Stores lanes 0 to 2 of each point at `out` (lane 3 is not written, as the original).
    fn store_points(&mut self, out: u32, points: &[Vec4]) {
        let mem = self;
        for (i, p) in points.iter().enumerate() {
            let at = out + 16 * i as u32;
            mem.set_f32(at, p.x);
            mem.set_f32(at + 4, p.y);
            mem.set_f32(at + 8, p.z);
        }
    }

    /// The drive line index at +0x17c, the word the drive line's thunk (`0x50a4f0`) takes.
    fn drive_line_word(&self, car: u32) -> u32 {
        let mem = self;
        mem.u32(car + 0x17C)
    }

    /// `E36Triple::MinPick` (0x4f3ec0) on the triple at `at`: the int pair and the float.
    fn min_pick(&self, at: u32) -> i32 {
        let mem = self;
        e36::min_pick(Triple { a: mem.u32(at) as i32, b: mem.u32(at + 4) as i32, f: mem.f32(at + 8) })
    }

    fn load_quat(&self, at: u32) -> Quat {
        let mem = self;
        let v = Vec4::load(mem, at);
        Quat { x: v.x, y: v.y, z: v.z, w: v.w }
    }

    fn store_quat(&mut self, at: u32, q: Quat) {
        let mem = self;
        (Vec4 { x: q.x, y: q.y, z: q.z, w: q.w }).store(mem, at);
    }

    fn zero_vec4(&mut self, at: u32) {
        let mem = self;
        (Vec4 { x: 0.0, y: 0.0, z: 0.0, w: 0.0 }).store(mem, at);
    }
}
