//! Body deformation and scrape from the physics damage (rbr-recomp `notes/car-damage-visuals.md`, section 3).
//!
//! The damage handler's 64 zone values (0 to 1, `handler+0x3808`) reach HedgeHog3D by `CSigCarDamage`, which copies them
//! into `g_carDamage` (`CHH_World::OnCarDamage`, HH 0x10066e90). Each damageable panel (`.sgc` subtype 3) has one list of
//! `(vertex, weight)` pairs per zone and one morph target, a delta per vertex. `CGeomObjectCar::UpdateDeformation`
//! (HH 0x10003b80, read from the asm) gives each vertex the weight `w = min(100, sum of weight * zone * 100)` over the
//! first 64 lists, moves it to `base + delta * w` (the holder at `+0x160` is the panel's `CHH_ModifierMorph`, and its
//! target 0's deltas are the offsets) and writes `w * 0.01` into the colour stream, which `CarPaint` and `CarPaintInt`
//! read as the scrape amount (`oD0.w`). Normals are not changed.
//!
//! Window cracks ([`Cracks`], section 4 of the note, from the HH asm): a `CSigCarDamage` with event code 3 or 4
//! (`CHH_Car::OnCarDamage`, HH 0x10016d50) projects `Textures\WS_Broken.dds` onto one window (`Unk16f00` 0x10016f00,
//! `ProjectUVs` 0x100165d0), and the car draws that window again with the projected uvs, blended by the texture's
//! alpha (HH 0x100161e0).

use crate::lights::Switch;
use glam::Vec3;
use std::sync::Arc;

/// Zone values in the damage handler and in `g_carDamage`.
pub const ZONES: usize = 64;
/// The most a vertex's weight reaches (`0x101b42a8`, 100.0), and the colour stream's scale of it (`w * 0.01`).
const WEIGHT_MAX: f32 = 100.0;
const SCRAPE_PER_WEIGHT: f32 = 0.01;

/// The damage handler's zone values, `0` to `1` each.
pub type Zones = [f32; ZONES];

/// The vertex weights of a panel with `vertex_count` vertices under `zones`, in HedgeHog3D's order: list `j` (of the
/// first 64) adds `weight * zones[j] * 100` to each of its vertices, each sum clamped to 100 after every add. An index
/// past the panel (none in retail files) is skipped.
pub fn vertex_weights(lists: &[Vec<(u32, f32)>], zones: &Zones, vertex_count: usize) -> Vec<f32> {
    let mut weights = vec![0.0f32; vertex_count];
    for (list, &zone) in lists.iter().zip(zones) {
        for &(index, weight) in list {
            if let Some(w) = weights.get_mut(index as usize) {
                *w += weight * zone * WEIGHT_MAX;
                if *w > WEIGHT_MAX {
                    *w = WEIGHT_MAX;
                }
            }
        }
    }
    weights
}

/// The scrape amount of a vertex weight: the colour stream's `w * 0.01`, 0 to 1.
pub fn scrape(weight: f32) -> f32 {
    weight * SCRAPE_PER_WEIGHT
}

/// A damageable panel's per-zone lists.
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub vertex_count: usize,
    pub lists: Vec<Vec<(u32, f32)>>,
}

/// What deformation does to one drawn mesh: its panel, and per drawn vertex the panel's vertex index and morph delta
/// (in the drawn mesh's space: the panel's placement applied to the model-space delta).
#[derive(Debug, Clone, PartialEq)]
pub struct Dents {
    pub panel: Arc<Panel>,
    pub vertices: Vec<(u32, Vec3)>,
}

impl Dents {
    /// Per drawn vertex, its offset `delta * w` and its scrape amount under `zones`.
    pub fn under(&self, zones: &Zones) -> Vec<(Vec3, f32)> {
        let weights = vertex_weights(&self.panel.lists, zones, self.panel.vertex_count);
        self.vertices
            .iter()
            .map(|&(index, delta)| {
                let w = weights.get(index as usize).copied().unwrap_or(0.0);
                (delta * w, scrape(w))
            })
            .collect()
    }
}

/// What damage does to one drawn car mesh: dents it when it is part of a damageable panel, or shows it when it is a
/// window's crack overlay and that crack is projected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshDamage {
    pub dents: Option<Dents>,
    pub crack: Option<Crack>,
    /// A lamp's part: drawn only in its lamp's state ([`crate::lights::Switch`]).
    pub lamp: Option<Switch>,
    /// The detachable part this mesh is, by its node name (`rbr_recording::detach::PARTS`).
    pub detach: Option<u32>,
    /// The wheel (0 to 3, `crate::playback`'s order) this mesh is, by its node name (`e_wheel_<class>_<side>` or its
    /// `e_wheel_<class>_tyre_<side>`).
    pub wheel: Option<usize>,
    /// A mirror's part, by its node name: with the cabin light its glass mirrors ([`crate::car::cabin::Mirror`]).
    pub mirror: Option<crate::car::cabin::Mirror>,
    /// The window slot ([`window_slot`]) of a window's part, by its node name: the pane seen from inside takes what its
    /// texture lets through (`crate::glass::Tints`).
    pub window: Option<usize>,
}

/// A scrape amount as the vertex colour's alpha byte.
pub fn scrape_byte(amount: f32) -> u8 {
    (amount.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The windows a crack can show on, by `g_HH_DmgParts` slot (HH 0x101d77c8): the exterior and interior part names.
pub const WINDOWS: [(&str, &str); 6] = [
    ("e_window_f", "i_window_f"),
    ("e_window_rf", "i_window_rf"),
    ("e_window_rb", "i_window_rb"),
    ("e_window_lf", "i_window_lf"),
    ("e_window_lb", "i_window_lb"),
    ("e_window_b", "i_window_b"),
];
/// Projections each window takes at most (`InitDamage`'s six calls of 0x1001bb70: 2, 1, 1, 1, 1, 2).
const CRACK_LIMITS: [usize; 6] = [2, 1, 1, 1, 1, 2];
/// The front and rear windows' slots. The others are side windows.
const FRONT: usize = 0;
const REAR: usize = 5;
/// `CSigCarDamage` event codes that crack a window (`ApplyFunctors`: `DmgCFA0` 3, `DmgCFE0` 4).
pub const CODE_FRONT: u32 = 3;
pub const CODE_SIDE_OR_REAR: u32 = 4;

/// The slot of the window part `name`, if it is one.
pub fn window_slot(name: &str) -> Option<usize> {
    WINDOWS.iter().position(|&(e, i)| e == name || i == name)
}

/// One crack projection: its window slot, and which of the two it is. On the front and rear windows `high` is the
/// right half of `WS_Broken.dds` (the projection's index is odd); on a side window it is the base 0.625 instead of
/// 0.375, which the game picks by `rand()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Crack {
    pub window: usize,
    pub high: bool,
}

/// The projections a window can take in the viewer. The front and rear windows take both halves. A side window
/// takes the base `rand()` picks, which the viewer fixes at the even draw (0.375).
pub fn crack_variants(window: usize) -> &'static [bool] {
    if window == FRONT || window == REAR {
        &[false, true]
    } else {
        &[false]
    }
}

/// The crack uv of a window vertex at `p`, whose window's vertices average `centre` (both in the car model's space:
/// `SetupDamagePart` transforms them by the part's world matrix at load). Front and rear (`ProjectUVs` 0x100165d0):
/// `u = (x - cx) * 0.6 + 0.375`, at most 0.5 (or `+ 0.625`, at least 0.5, for `high`), then `1 - u` when the car's
/// `mirror` bit is set (`m_flags15c` or `m_flags160` bit 0, `rand()` in `InitDamage`); side windows
/// `u = (z - cz) * 0.8 + base`. Both have `v = 0.5 - (y - cy) * 2`.
pub fn crack_uv(crack: Crack, mirror: bool, centre: Vec3, p: Vec3) -> [f32; 2] {
    let base = if crack.high { 0.625 } else { 0.375 };
    let v = 0.5 - (p.y - centre.y) * 2.0;
    if crack.window != FRONT && crack.window != REAR {
        return [(p.z - centre.z) * 0.8 + base, v];
    }
    let u = (p.x - centre.x) * 0.6 + base;
    let u = if crack.high { u.max(0.5) } else { u.min(0.5) };
    [if mirror { 1.0 - u } else { u }, v]
}

/// The crack projections a car has taken (`CHH_Car::OnCarDamage` and `Unk16f00`). The front and rear windows take
/// each half once (`m_flags15c` and `m_flags160` bits 2 and 4); every window takes at most its limit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cracks {
    /// The windows the car has a part for.
    present: [bool; 6],
    /// Per window, the projections taken, in order.
    taken: [Vec<bool>; 6],
}

impl Cracks {
    /// No cracks, on a car with parts for the windows `present`.
    pub fn new(present: [bool; 6]) -> Cracks {
        Cracks { present, taken: Default::default() }
    }

    /// Whether `crack` has been projected.
    pub fn shows(&self, crack: Crack) -> bool {
        self.taken[crack.window].contains(&crack.high)
    }

    /// A `CSigCarDamage` with event code `code` and functor index `index` (the record's words 0 and 2). Code 3
    /// cracks the front window, the index's parity choosing the half. Code 4 cracks side window `index + 1` (index 0
    /// to 3: right front, right back, left front, left back), or for index 4 the rear window, both halves. Other
    /// codes do nothing; an index past the table (out of bounds in the game) is ignored.
    pub fn on_damage(&mut self, code: u32, index: u32) {
        match (code, index) {
            (CODE_FRONT, _) => self.project(FRONT, index % 2 == 1),
            (CODE_SIDE_OR_REAR, 4) => {
                self.project(REAR, false);
                self.project(REAR, true);
            }
            (CODE_SIDE_OR_REAR, 0..=3) => self.project(index as usize + 1, false),
            _ => {}
        }
    }

    fn project(&mut self, window: usize, high: bool) {
        let taken = &mut self.taken[window];
        let halves = window == FRONT || window == REAR;
        if !self.present[window] || taken.len() >= CRACK_LIMITS[window] || halves && taken.contains(&high) {
            return;
        }
        taken.push(high);
    }
}

/// Parses damage events written as `code:index` pairs separated by commas or whitespace (`3:0 4:4`).
pub fn parse_events(text: &str) -> Result<Vec<(u32, u32)>, String> {
    pairs(text)
        .map(|(code, index)| {
            Ok((code.parse().map_err(|e| format!("{code}: {e}"))?, index.parse().map_err(|e| format!("{index}: {e}"))?))
        })
        .collect()
}

/// The `a:b` pairs of `text`, separated by commas or whitespace; a pair without a colon is `(pair, "")`.
fn pairs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split([',', ' ', '\n', '\t']).filter(|p| !p.is_empty()).map(|p| p.split_once(':').unwrap_or((p, "")))
}

/// Parses zone values written as `zone:value` pairs separated by commas or whitespace (`1:1.0,3:0.43`), zones not
/// named being 0.
pub fn parse_zones(text: &str) -> Result<Zones, String> {
    let mut zones = [0.0; ZONES];
    for (zone, value) in pairs(text) {
        let zone: usize = zone.parse().map_err(|e| format!("{zone}: {e}"))?;
        let value: f32 = value.parse().map_err(|e| format!("zone {zone}: {value:?}: {e}"))?;
        *zones.get_mut(zone).ok_or_else(|| format!("zone {zone}: zones are 0 to {}", ZONES - 1))? = value;
    }
    Ok(zones)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zones(pairs: &[(usize, f32)]) -> Zones {
        let mut z = [0.0; ZONES];
        for &(i, v) in pairs {
            z[i] = v;
        }
        z
    }

    #[test]
    fn weights_sum_over_zones_and_clamp_at_100() {
        // Vertex 0 is in zones 1 and 2, vertex 1 in zone 2 only, vertex 2 in zone 3, which is undamaged.
        let lists = vec![vec![], vec![(0, 0.5)], vec![(0, 1.0), (1, 0.25)], vec![(2, 1.0)]];
        let w = vertex_weights(&lists, &zones(&[(1, 0.5), (2, 0.4)]), 4);
        assert_eq!(w, vec![0.5 * 0.5 * 100.0 + 0.4 * 100.0, 0.25 * 0.4 * 100.0, 0.0, 0.0]);
        // Full damage in both zones: 50 + 100 clamps to 100.
        let w = vertex_weights(&lists, &zones(&[(1, 1.0), (2, 1.0)]), 4);
        assert_eq!(w[0], 100.0);
        assert_eq!(w[1], 25.0);
    }

    #[test]
    fn only_the_first_64_lists_count_and_bad_indices_are_skipped() {
        let mut lists = vec![Vec::new(); ZONES + 1];
        lists[ZONES] = vec![(0, 1.0)];
        lists[5] = vec![(9, 1.0), (1, 1.0)];
        let w = vertex_weights(&lists, &[1.0; ZONES], 2);
        assert_eq!(w, vec![0.0, 100.0]);
    }

    #[test]
    fn dents_move_each_vertex_by_its_delta_times_the_weight() {
        // One panel of three vertices; drawn vertex 0 is panel vertex 2 and drawn vertex 1 panel vertex 0.
        let panel = Arc::new(Panel { vertex_count: 3, lists: vec![vec![], vec![(0, 0.5), (2, 1.0)]] });
        let dents = Dents { panel, vertices: vec![(2, Vec3::new(0.0, -0.002, 0.0)), (0, Vec3::new(0.001, 0.0, 0.0))] };
        let moved = dents.under(&zones(&[(1, 0.8)]));
        // Weights: panel vertex 2 is 1.0 * 0.8 * 100 = 80, vertex 0 is 0.5 * 0.8 * 100 = 40.
        assert!((moved[0].0 - Vec3::new(0.0, -0.16, 0.0)).length() < 1e-6);
        assert!((moved[0].1 - 0.8).abs() < 1e-6);
        assert!((moved[1].0 - Vec3::new(0.04, 0.0, 0.0)).length() < 1e-6);
        assert!((moved[1].1 - 0.4).abs() < 1e-6);
        // No damage: no offset and no scrape.
        assert!(dents.under(&[0.0; ZONES]).iter().all(|&(o, s)| o == Vec3::ZERO && s == 0.0));
        assert_eq!(scrape_byte(0.4), 102);
    }

    #[test]
    fn cracks_follow_the_event_codes_and_limits() {
        let mut cracks = Cracks::new([true, true, true, true, false, true]);
        let front = |high| Crack { window: 0, high };
        cracks.on_damage(3, 2);
        assert!(cracks.shows(front(false)) && !cracks.shows(front(true)));
        // The same half again does nothing; the odd index takes the other half.
        cracks.on_damage(3, 4);
        cracks.on_damage(3, 1);
        assert_eq!(cracks.taken[0], vec![false, true]);
        // Code 4: index 1 is the right back window, once; index 3 the left back, which this car lacks.
        cracks.on_damage(4, 1);
        cracks.on_damage(4, 1);
        cracks.on_damage(4, 3);
        assert_eq!(cracks.taken[2], vec![false]);
        assert!(cracks.taken[4].is_empty());
        // Index 4 is the rear window, both halves at once; code 0 and other codes do nothing.
        cracks.on_damage(4, 4);
        cracks.on_damage(0, 4);
        cracks.on_damage(4, 9);
        assert_eq!(cracks.taken[5], vec![false, true]);
    }

    #[test]
    fn crack_uvs_project_across_the_window() {
        let close = |got: [f32; 2], want: [f32; 2]| {
            assert!((got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6, "{got:?} {want:?}")
        };
        let centre = Vec3::new(0.0, 1.0, -1.0);
        let front = Crack { window: 0, high: false };
        close(crack_uv(front, false, centre, Vec3::new(-0.25, 1.1, -1.2)), [0.225, 0.3]);
        // The left half stops at u = 0.5, the right half starts there, and the mirror bit flips u.
        close(crack_uv(front, false, centre, Vec3::new(0.5, 1.0, -1.0)), [0.5, 0.5]);
        let high = Crack { high: true, ..front };
        close(crack_uv(high, false, centre, Vec3::new(-0.5, 1.0, -1.0)), [0.5, 0.5]);
        close(crack_uv(high, true, centre, Vec3::new(0.0, 1.0, -1.0)), [0.375, 0.5]);
        // A side window maps along the car (z), without a clamp.
        let side = Crack { window: 3, high: false };
        close(crack_uv(side, true, centre, Vec3::new(0.7, 0.75, -0.5)), [0.775, 1.0]);
        assert_eq!(window_slot("i_window_lb"), Some(4));
        assert_eq!(window_slot("e_window_b"), Some(5));
        assert_eq!(window_slot("e_body"), None);
    }

    #[test]
    fn zones_parse_from_pairs() {
        let z = parse_zones("1:1.0, 3:0.25\n63:0.5").unwrap();
        assert_eq!((z[1], z[3], z[63], z[0]), (1.0, 0.25, 0.5, 0.0));
        assert!(parse_zones("64:1").is_err());
        assert!(parse_zones("3").is_err());
        assert_eq!(parse_events("3:0, 4:4").unwrap(), vec![(3, 0), (4, 4)]);
        assert!(parse_events("3").is_err());
    }
}
