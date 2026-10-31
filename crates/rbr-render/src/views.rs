//! The camera views of a car in playback and live mode (`viewer --view`, the C key): in-car, bonnet, bumper and chase
//! (the user's set, 2026-10-18), each nudged per car by the player ([`crate::camera_prefs`]).
//!
//! Each car's `<car>.ini` has `Cam_*` sections (`notes/formats.md` "Cars"): `Pos`, `Target` and `Up` in the car
//! model's frame (front `-z`, left `+x`, up `+y`), `FOV` in radians and `Near` in metres. `E09_LoadCarCameras`
//! (0x469210) reads `Cam_bonnet`, `Cam_internal` and `Cam_bonnet2`. An RSF car's `<car>_personal.ini` beside it
//! replaces those sections ([`crate::car::camera_sections`]). The viewer takes the place, the target and the near
//! plane, and not `FOV`: the in-car, bonnet and bumper views have the horizontal field of view of `viewer --fov`
//! (default [`crate::coords::FOV_DEGREES`]), the chase its own fixed one ([`crate::playback::CHASE_FOV_DEGREES`]).
//! The vertical one follows from the window's aspect ([`crate::coords::vertical_fov`]).
//!
//! - In-car: `Cam_internal` (the game's mode 5, `M5c50` 0x485c50), moved back by [`COCKPIT_BACK`] (ours), so the
//!   dash and the steering wheel are in frame, or less where the seat or the cage would then block or enter the view
//!   ([`cockpit_back`], ours), and at least [`WHEEL_BEHIND`] the steering wheel, looking at least [`COCKPIT_PITCH`]
//!   down (ours), and for the RSF models' narrow `FOV` down to the wheel's hub ([`hub_in_view`], ours). It draws the interior set (`i_*`), as
//!   `CHH_Car::Render` does in the in-car views (rbr-recomp `notes/formats/sgc.md`, section 9), and takes the game's seat shake
//!   ([`Mount::shaken`]).
//! - Bonnet: ours, low over the bonnet's rear, where the windscreen rises: found by walking back from `Cam_bonnet`'s
//!   `Pos` (the game's mode 3, an eye at the bonnet's front edge from which the bonnet is out of frame) along the
//!   body's top, [`BONNET_CLEARANCE`] above it ([`bonnet_mount`]), looking along `Cam_bonnet`. It draws the exterior body; the stage probe keeps it off the
//!   ground (`M5760`'s, [`crate::game_camera::bonnet_lift`]).
//! - Bumper: ours. No section places it; the eye is [`BUMPER_AHEAD`] in front of the model's front and
//!   [`BUMPER_HEIGHT`] above its lowest point, looking straight ahead.
//! - Chase: the game's chase camera (mode 1, [`crate::game_camera`]: its lag, its height window and its probes
//!   against the stage) with our framing ([`crate::game_camera::Settings::default`]).
//! - Start orbit: in live play from the stage start, the game's countdown camera (mode 10,
//!   `rbr_gameplay::camera_go`) replaces the view until the go: [`start_orbit`].
//! - Helicam: while the rescue helicopter runs, its camera (mode 8) replaces the view and C does nothing, as the
//!   game's CH_CAM (0x487840) is blocked then: [`Shown`].
//!
//! The game's C order is chase, `Cam_bonnet`, `Cam_bonnet2`, `Cam_internal` (rbr-recomp `notes/cameras.md` 2b). Ours is in-car,
//! bonnet, bumper, chase; the game's bonnet 2 (`Cam_bonnet2`, drawn with the interior set) is dropped (user).

use crate::camera::Lens;
use crate::car;
use crate::coords;
use crate::parallel::in_parallel;
use crate::playback::{ChaseLook, Playback};
use crate::stage::Mesh;
use glam::Vec3;
use rbr_formats::car_parts;
use rbr_formats::data::Data;
use rbr_formats::ini::Section;
use rbr_recording::camera::Camera;

/// The bumper eye's distance in front of the model's front, m. Ours.
pub const BUMPER_AHEAD: f32 = 0.05;
/// The bumper eye's height above the model's lowest point (the tyres' bottom), m. Ours.
pub const BUMPER_HEIGHT: f32 = 0.45;
/// Without a usable `Cam_bonnet`, the bonnet search starts this far in front of `Cam_internal`'s eye and below it, m,
/// looking along [`BONNET_LOOK`] (about 3 degrees down, as the retail `Cam_bonnet`s). Ours.
pub const BONNET_AHEAD_OF_INTERNAL: f32 = 1.4;
pub const BONNET_BELOW_INTERNAL: f32 = 0.2;
pub const BONNET_LOOK: Vec3 = Vec3::new(0.0, -0.15, -2.8);
/// A `Cam_bonnet` less than this in front of `Cam_internal`, m, is not at the bonnet (ours): the RSF Fiesta WRC 14's
/// sits over its windscreen.
const BONNET_MIN_AHEAD: f32 = 0.8;
/// The bonnet eye's height above the bonnet's rear, m. Ours: low, so that the bonnet fills the bottom of the image at
/// 50 degrees across (seen from higher up, it lies about 20 degrees below the horizon).
pub const BONNET_CLEARANCE: f32 = 0.08;
/// And at least this above what lies in the middle under it (a cowl, the wiper arms), m. Ours.
pub const BONNET_MIDDLE_CLEARANCE: f32 = 0.02;
/// [`bonnet_mount`] follows the body's top back from `Cam_bonnet` in these steps, m, at most [`BONNET_SEARCH`] m, and
/// puts the bonnet's rear where the rise over the next [`BONNET_SPAN`] steps grows most over the rise of the last ones:
/// the windscreen's foot. A slope alone does not tell them apart: the RSF DS3 R1's bonnet rises as steeply as
/// IMPREZA00's windscreen.
const BONNET_STEP: f32 = 0.05;
const BONNET_SEARCH: f32 = 2.0;
/// The walk starts this far in front of `Cam_bonnet`, m: the RSF 207 S2000's is 0.3 m behind its bonnet's front edge,
/// and the Escort Mk2's in front of its nose.
const BONNET_START: f32 = 0.5;
const BONNET_SPAN: usize = 5;
/// The bonnet view looks down far enough to put the bonnet's front edge this far below the middle of the image,
/// degrees, never higher than `Cam_bonnet` and at most [`BONNET_TILT_MAX`] lower. Ours: from a low eye the bonnet lies 12 to 16 degrees down, at the
/// bottom edge of 50 degrees across at 16:9.
const BONNET_EDGE_BELOW: f32 = 10.0;
/// The most that rule tilts the view below `Cam_bonnet`'s, degrees (ours).
const BONNET_TILT_MAX: f32 = 8.0;
/// Below `Cam_bonnet`'s height by more than this, m, the walk is still in front of the bonnet (ACCENT's `Pos` is over
/// its bumper, 0.26 m above it) and climbs on.
const BONNET_BELOW: f32 = 0.2;
/// The bonnet's front edge is the first point of the walk less than this below its rear, m (ours; ACCENT's bumper is
/// 0.26 m below its bonnet).
const BONNET_FRONT_DROP: f32 = 0.25;
/// Where [`bonnet_mount`] follows the bonnet across the car (model `x`), m: either side of a bonnet scoop (IMPREZA00's
/// rises steeply in the middle), and where it measures the eye's height over the middle.
const BONNET_SIDES: [f32; 2] = [-0.4, 0.4];
const BONNET_MIDDLE: [f32; 3] = [-0.2, 0.0, 0.2];
/// How far the cockpit eye sits behind `Cam_internal`'s `Pos`, m (along `+z`). Ours. At 0.4 only the top of the
/// steering wheel shows at 50 degrees horizontal (COROLLA); at 0.5 the wheel, the dash and the pillars do.
pub const COCKPIT_BACK: f32 = 0.5;
/// The cockpit view looks at least this far down, degrees (ours): the retail cars' `Cam_internal`s look 2.8 to 5.3
/// degrees down, and some RSF models' level or up (the Alpine A110's 0.2 degrees up, the Fiesta Rally3 Evo's 2.4),
/// which at 50 degrees across shows the dash top and the roof only.
pub const COCKPIT_PITCH: f32 = 2.8;
/// [`cockpit_back`] tries the eye back in these steps from [`COCKPIT_BACK`], m. Ours.
const COCKPIT_STEP: f32 = 0.05;
/// The cockpit eye starts at least this far behind the steering wheel (the `i_steeringwheel` node), m, along `+z`.
/// Ours: the retail cars' eyes, [`COCKPIT_BACK`] behind `Pos`, are 0.66 (PEUGEOT_206) to 0.85 m behind it. Some RSF
/// models' `Pos` lies over the dash, at or in front of the wheel (the Fiesta Rally3's 0.17 m, the Fiesta Rally2's
/// 0.25 m in front of it): from there the dash top hides the wheel and the dials.
pub const WHEEL_BEHIND: f32 = 0.65;
/// The most [`WHEEL_BEHIND`] moves the eye back from `Pos`, m. Ours.
const COCKPIT_BACK_MAX: f32 = 1.0;
/// A car whose `Cam_internal` `FOV` is narrower than this, radians, is shown with its wheel's hub at most
/// [`HUB_BELOW`] under the bottom of the default image ([`hub_in_view`]). Ours: every retail car's is 1.308997 (75
/// degrees), the RSF models' 0.98 to 1.0006, and their displays and shift lights sit on the wheel or on the column
/// inside its rim (the Fiesta Rally3's, the 208 Rally4's) instead of above it.
const NARROW_FOV: f32 = 1.2;
/// The default image's lower half, degrees: [`coords::FOV_DEGREES`] across at 16:9.
const HALF_VERTICAL: f32 = 14.7;
/// How far under the default image's bottom the wheel's hub may lie, degrees (ours: then the top half of the wheel and
/// what lies in it show), and the most [`hub_in_view`] tilts the view for it, degrees.
const HUB_BELOW: f32 = 3.0;
const HUB_TILT_MAX: f32 = 5.0;
const HUB_TILT_STEP: f32 = 0.5;
/// The tilt stops before a surface the view shows comes nearer than this, m. Ours.
const HUB_CLEAR: f32 = 0.3;
/// The view is blocked when a surface it shows is nearer than this along one of the lines [`COCKPIT_LOOK`], m. Ours.
pub const COCKPIT_CLEAR: f32 = 0.6;
/// The lines [`cockpit_back`] looks along: about 27 by 14 degrees around the view's middle, as (right, up) offsets
/// per metre ahead.
const COCKPIT_LOOK: [f32; 5] = [-0.24, -0.12, 0.0, 0.12, 0.24];
const COCKPIT_LOOK_UP: [f32; 3] = [-0.12, 0.0, 0.12];
/// [`cockpit_back`] also keeps every interior surface the view shows at least this far from the eye across the
/// default frame, m. Ours: where they enter it, the RSF models' seat cushions and shells and the cage tubes and visors
/// beside the eye lie 0.11 to 0.32 m from it (the Audi quattro A2's red bolsters 0.26 to 0.3 m), and the dashes,
/// wheels, doors and roofs that show 0.39 m (the Alpine A110's roof) or more, the retail cars' 0.42 m or more
/// (`examples/cockpit_survey.rs`).
pub const FRAME_CLEAR: f32 = 0.35;
/// The frame is also checked from the eye this far lower, m: the seat shake's dip. Ours: the in-car views' shake
/// ([`Mount::shaken`]) rests 0.012 m up; in 0.1% of the frames it goes below -0.019 m (377k frames of the retail
/// recordings) and -0.026 m (300k of an NGP autopilot drive of the RSF Audi quattro A2 on track-45_N), and to -0.08 and
/// -0.1 m only in landings and crashes.
const SHAKE_DIP: f32 = 0.03;
/// The lines [`frame_clear`] looks along: across the default frame, edges included.
const FRAME_COLUMNS: usize = 9;
const FRAME_ROWS: usize = 5;

/// A camera view of the car.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CarView {
    Cockpit,
    Bonnet,
    Bumper,
    #[default]
    Chase,
}

impl CarView {
    /// The views in the order the C key cycles them.
    pub const ALL: [CarView; 4] = [CarView::Cockpit, CarView::Bonnet, CarView::Bumper, CarView::Chase];

    /// The `--view` name.
    pub fn name(self) -> &'static str {
        match self {
            CarView::Cockpit => "cockpit",
            CarView::Bonnet => "bonnet",
            CarView::Bumper => "bumper",
            CarView::Chase => "chase",
        }
    }

    /// The view named `name` (ignoring case).
    pub fn parse(name: &str) -> Option<CarView> {
        CarView::ALL.into_iter().find(|v| v.name().eq_ignore_ascii_case(name))
    }

    /// The view after this one.
    pub fn next(self) -> CarView {
        let i = CarView::ALL.iter().position(|&v| v == self).unwrap_or(0);
        CarView::ALL[(i + 1) % CarView::ALL.len()]
    }

    /// The view's place in [`CarView::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Whether the view draws the interior set instead of the exterior: the cockpit.
    pub fn interior(self) -> bool {
        self == CarView::Cockpit
    }

    /// Whether the view draws the driver and co-driver. HedgeHog3D's renderers draw the crew's list (world `+0x200`)
    /// only outside the in-car modes and mode 3 (the `RenderQuality = high` default, 0x100763f0: not with the
    /// render options' in-car flag, set for modes 4 to 6 by 0x10066d20, and not in camera mode 2, the bonnet). The
    /// bumper view is ours and goes with the bonnet.
    pub fn shows_crew(self) -> bool {
        self == CarView::Chase
    }

    /// Whether the view takes the seat shake (the game's mode 5).
    pub fn shakes(self) -> bool {
        self == CarView::Cockpit
    }
}

/// What the viewer shows: the player's view, or a camera the game forces over it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shown {
    Player(CarView),
    /// The start orbit (mode 10) at its swing angle: the game chase turned.
    StartOrbit(f32),
    /// The rescue helicopter's camera (mode 8).
    Helicam,
    /// The car-flip scene's track-side camera (mode 13, `FUN_00487980`).
    FlipCam,
}

impl Shown {
    /// The start orbit while the live camera record says it runs, else the helicam while `helicopter` is active,
    /// else the flip camera while `flip` is, else the player's `view`.
    pub fn of(view: CarView, camera: Option<Camera>, helicopter: bool, flip: bool) -> Shown {
        match start_orbit(camera) {
            Some(angle) => Shown::StartOrbit(angle),
            None if helicopter => Shown::Helicam,
            None if flip => Shown::FlipCam,
            None => Shown::Player(view),
        }
    }

    /// Whether `other` is the same camera: the start orbit at any swing angle is one.
    pub fn same_camera(self, other: Shown) -> bool {
        match (self, other) {
            (Shown::Player(a), Shown::Player(b)) => a == b,
            _ => std::mem::discriminant(&self) == std::mem::discriminant(&other),
        }
    }

    /// The car view whose set is drawn and whose lens is used: the chase (outside) for the forced cameras.
    pub fn car_view(self) -> CarView {
        match self {
            Shown::Player(view) => view,
            Shown::StartOrbit(_) | Shown::Helicam | Shown::FlipCam => CarView::Chase,
        }
    }

    /// The start orbit's swing angle while it runs.
    pub fn swing(self) -> Option<f32> {
        match self {
            Shown::StartOrbit(angle) => Some(angle),
            _ => None,
        }
    }

    /// The camera shown: `scripted` (the characters' mode-8 or mode-13 lens) while it runs, else `view`'s through
    /// `playback`, with the chase turned by `look` outside the start orbit.
    pub fn lens(self, playback: &Playback, mounts: &Mounts, scripted: Option<Lens>, look: ChaseLook, fov: f32) -> Lens {
        match (self, scripted) {
            (Shown::Helicam | Shown::FlipCam, Some(lens)) => lens,
            (Shown::StartOrbit(_), _) => playback.lens(mounts, CarView::Chase, ChaseLook::default(), fov),
            _ => playback.lens(mounts, self.car_view(), look, fov),
        }
    }

    /// The name the HUD shows.
    pub fn name(self) -> &'static str {
        match self {
            Shown::Player(view) => view.name(),
            Shown::StartOrbit(_) => "start orbit",
            Shown::Helicam => "helicam",
            Shown::FlipCam => "flip camera",
        }
    }

    /// Whether C changes the player's view: not while the helicam or the flip camera runs (`CH_CAM` 0x487840 returns
    /// while the helicopter is active or in mode 13).
    pub fn may_cycle(self) -> bool {
        !matches!(self, Shown::Helicam | Shown::FlipCam)
    }
}

/// The game's start orbit's swing angle while the live process's camera record (`rbr_recording::camera`) says it
/// runs, or `None`: then the player's view. The game (`E15CamHost::M8840`, mode 10) turns the chase offset by the
/// angle about the up axis of its z-up physics frame, from pi (in front of the car) down to 0 (behind it):
/// [`crate::game_camera::Chase::tic`].
pub fn start_orbit(camera: Option<Camera>) -> Option<f32> {
    camera?.start_orbit()
}

/// A camera fixed to the car, in the car model's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mount {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    /// Metres.
    pub near: f32,
}

impl Mount {
    /// The camera moved by the seat shake `shake` (`CSigIncarShake` +0x30, the driver's seat spring at `CPassenger`'s
    /// host +0x70), a body-frame vector (x left, y rear, z up): `M5120` and `M5c50` add it to the camera's place in
    /// the car (host `+0x4e0`, set by 0x484970).
    pub fn shaken(self, shake: Vec3) -> Mount {
        let shift = Vec3::new(shake.x, shake.z, shake.y);
        Mount { eye: self.eye + shift, target: self.target + shift, ..self }
    }

    /// The camera moved `metres` back along the car's rear (`+z`; the front is `-z`), with its target: the view
    /// direction is kept.
    pub fn moved_back(self, metres: f32) -> Mount {
        self.moved(Vec3::Z * metres)
    }

    /// The camera and its target moved by `shift`, model frame.
    pub fn moved(self, shift: Vec3) -> Mount {
        Mount { eye: self.eye + shift, target: self.target + shift, ..self }
    }

    /// The camera turned to look at least `degrees` down, about its eye, or as it is when it does.
    pub fn looking_down(self, degrees: f32) -> Mount {
        let look = self.target - self.eye;
        let flat = Vec3::new(look.x, 0.0, look.z);
        if look.y.atan2(flat.length()) <= -degrees.to_radians() {
            return self;
        }
        let (sin, cos) = (-degrees.to_radians()).sin_cos();
        let target = self.eye + (flat.normalize_or(Vec3::NEG_Z) * cos + Vec3::Y * sin) * look.length();
        Mount { target, ..self }
    }

    /// The camera turned `degrees` up about its eye (down when negative), the target as far.
    pub fn tilted(self, degrees: f32) -> Mount {
        if degrees == 0.0 {
            return self;
        }
        let look = self.target - self.eye;
        let flat = Vec3::new(look.x, 0.0, look.z);
        let (sin, cos) = (look.y.atan2(flat.length()) + degrees.to_radians()).sin_cos();
        let target = self.eye + (flat.normalize_or(Vec3::NEG_Z) * cos + Vec3::Y * sin) * look.length();
        Mount { target, ..self }
    }

    /// The camera moved and tilted by the player's `nudge` ([`crate::camera_prefs`]), after the view's own rules.
    pub fn nudged(self, nudge: Nudge) -> Mount {
        let shift = nudge.shift;
        self.moved(Vec3::new(-shift.x, shift.y, -shift.z)).tilted(nudge.tilt)
    }

    /// A `Cam_*` section's camera, or `None` when a key is missing or not numbers. Upright: `E09_BuildCamMatrix`
    /// (0x4663f0) turns the camera by `Pos` and `Target` alone and never reads the `Up` the loader passes it (the BMW
    /// M3 E30 and Alpine A110 RSF models have `Up = 0 0.049 0.999`, which would roll the view on its side).
    pub fn from_section(section: &Section) -> Option<Mount> {
        let vector = |key| -> Option<Vec3> {
            let v = section.get(key)?.split_whitespace().map(str::parse).collect::<Result<Vec<f32>, _>>().ok()?;
            Some(Vec3::from_slice(v.get(..3)?))
        };
        let number = |key| section.get(key)?.trim().parse::<f32>().ok();
        Some(Mount { eye: vector("Pos")?, target: vector("Target")?, up: Vec3::Y, near: number("Near")? })
    }
}

/// The player's change to a view of a car ([`crate::camera_prefs`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Nudge {
    /// Right, up and forward, m, in the car's frame.
    pub shift: Vec3,
    /// Degrees up (the fixed views').
    pub tilt: f32,
}

/// A car's fixed views, in its model frame, and the player's nudges of every view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mounts {
    pub bonnet: Mount,
    pub bumper: Mount,
    pub cockpit: Mount,
    /// Each view's nudge, by [`CarView::index`] ([`crate::camera_prefs`]). The chase's shift is applied through its
    /// settings ([`crate::game_camera::Settings::nudged`]), not here.
    pub nudges: [Nudge; CarView::ALL.len()],
}

impl Mounts {
    /// The views from `<car>.ini`'s sections, with the bumper placed by the box `lo`..`hi` around the model's
    /// `exterior` meshes, the bonnet on their top and the cockpit behind the steering `wheel` and clear of the
    /// `interior` set's seat (model frame).
    pub fn from_sections(
        sections: &[Section],
        lo: Vec3,
        hi: Vec3,
        exterior: &[Mesh],
        interior: &[Mesh],
        wheel: Option<Vec3>,
    ) -> Result<Mounts, String> {
        let mount = |name| {
            car_parts::section(sections, name)
                .and_then(Mount::from_section)
                .ok_or_else(|| format!("<car>.ini: no camera [{name}] with Pos, Target, Up and Near"))
        };
        let eye = Vec3::new((lo.x + hi.x) * 0.5, lo.y + BUMPER_HEIGHT, lo.z - BUMPER_AHEAD);
        let internal = mount("Cam_internal")?;
        // To give the bonnet view "internal cockpit cam values without shaking", NGPCarMenu empties `[Cam_bonnet]`'s
        // Pos, Target and Near (its `NGPCarMenuComment` there, RSF's Xsara Kit Car), and some RSF cars copy
        // `[Cam_internal]` there, keeping the model's own as `Backup_Pos`, `Backup_Target` and `Backup_Near` (the Fiat
        // 131 Abarth). Then the bonnet takes the backup, or else is placed from the internal camera (ours).
        let at_front = |m: &Mount| m.eye.z < internal.eye.z - BONNET_MIN_AHEAD;
        let backup = car_parts::section(sections, "Cam_bonnet").and_then(|section| {
            let entries =
                section.entries.iter().filter_map(|(k, v)| Some((k.strip_prefix("Backup_")?.to_owned(), v.clone())));
            Mount::from_section(&Section { name: section.name.clone(), entries: entries.collect() })
        });
        let bonnet = mount("Cam_bonnet").ok().filter(at_front).or(backup.filter(at_front)).unwrap_or_else(|| {
            let eye = internal.eye + Vec3::new(-internal.eye.x, -BONNET_BELOW_INTERNAL, -BONNET_AHEAD_OF_INTERNAL);
            Mount { eye, target: eye + BONNET_LOOK, ..internal }
        });
        let fov = car_parts::section(sections, "Cam_internal").and_then(|s| s.get("FOV")?.trim().parse::<f32>().ok());
        let narrow = fov.is_some_and(|fov| fov < NARROW_FOV);
        let cockpit = internal.looking_down(COCKPIT_PITCH);
        let cockpit = cockpit.moved_back(cockpit_back(cockpit, interior, wheel, narrow));
        Ok(Mounts {
            bonnet: bonnet_mount(bonnet, exterior),
            bumper: Mount { eye, target: eye + Vec3::NEG_Z, up: Vec3::Y, near: coords::NEAR.min(BUMPER_AHEAD) },
            cockpit: wheel.filter(|_| narrow).map_or(cockpit, |wheel| hub_in_view(cockpit, wheel, &shown(interior))),
            nudges: [Nudge::default(); CarView::ALL.len()],
        })
    }

    /// Car `name`'s views (see [`Mounts::from_sections`]).
    pub fn load(
        data: &Data,
        name: &str,
        lo: Vec3,
        hi: Vec3,
        exterior: &[Mesh],
        interior: &[Mesh],
    ) -> Result<Mounts, String> {
        let sections = car::camera_sections(data, name).map_err(|e| e.to_string())?;
        let wheel = car::node_place(data, name, "i_steeringwheel").map_err(|e| e.to_string())?;
        Mounts::from_sections(&sections, lo, hi, exterior, interior, wheel)
    }

    /// The mount of a fixed view with its nudge, or `None` for the chase.
    pub fn of(&self, view: CarView) -> Option<Mount> {
        let mount = match view {
            CarView::Chase => return None,
            CarView::Bonnet => self.bonnet,
            CarView::Bumper => self.bumper,
            CarView::Cockpit => self.cockpit,
        };
        Some(mount.nudged(self.nudges[view.index()]))
    }
}

/// The bonnet eye: from `place` (`Cam_bonnet`, at the bonnet's front edge) back along the top of the `exterior`
/// meshes (rays straight down along [`BONNET_SIDES`]) onto the bonnet and to its rear, where the windscreen rises
/// ([`BONNET_SPAN`]),
/// and [`BONNET_CLEARANCE`] above the sides there ([`BONNET_MIDDLE_CLEARANCE`] above [`BONNET_MIDDLE`]), looking the way
/// `place` does, tilted down by [`BONNET_EDGE_BELOW`]'s rule. `place` as it is when nothing is under it. Ours.
pub fn bonnet_mount(place: Mount, exterior: &[Mesh]) -> Mount {
    // The exterior triangles that a ray down at model `x` may hit: those across it.
    let column = |x: f32| {
        let across = |t: &[Vec3; 3]| t.iter().any(|p| p.x <= x + 0.01) && t.iter().any(|p| p.x >= x - 0.01);
        let triangles = exterior.iter().flat_map(|mesh| {
            mesh.indices
                .chunks_exact(3)
                .map(|t| [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.vertices[i as usize].position)))
        });
        (x, triangles.filter(across).collect::<Vec<_>>())
    };
    let top = |z: f32, columns: &[(f32, Vec<[Vec3; 3]>)]| {
        let highest = columns
            .iter()
            .flat_map(|(x, triangles)| {
                let from = Vec3::new(*x, place.eye.y + 2.0, z);
                triangles.iter().filter_map(move |&t| ray_hit(from, Vec3::NEG_Y, t).map(|d| from.y - d))
            })
            .fold(f32::NEG_INFINITY, f32::max);
        highest.is_finite().then_some(highest)
    };
    let sides = BONNET_SIDES.map(column);
    // From [`BONNET_START`] in front of `place`, at the first step with the body under it.
    let start = place.eye.z - BONNET_START;
    let steps: Vec<usize> = (0..=(BONNET_SEARCH / BONNET_STEP) as usize).collect();
    // Every step's top at once (each casts its rays at the triangles of both sides' columns).
    let tops = in_parallel(&steps, |&i| top(start + i as f32 * BONNET_STEP, &sides));
    let Some(first) = tops.iter().position(Option::is_some) else {
        return place;
    };
    let z_at = |i: usize| start + (first + i) as f32 * BONNET_STEP;
    let profile: Vec<f32> = tops[first..].iter().map_while(|&top| top).collect();
    let bend = |i: usize| (profile[i + BONNET_SPAN] - profile[i]) - (profile[i] - profile[i - BONNET_SPAN]);
    let rear = (BONNET_SPAN..profile.len().saturating_sub(BONNET_SPAN))
        .filter(|&i| profile[i] >= place.eye.y - BONNET_BELOW)
        .max_by(|&a, &b| bend(a).total_cmp(&bend(b)))
        .unwrap_or(profile.len() - 1);
    // A seam or a cowl just behind the bonnet is lower than its rear.
    let height = profile[rear.saturating_sub(2)..=rear].iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let z = z_at(rear);
    let middle = top(z, &BONNET_MIDDLE.map(column)).map_or(f32::NEG_INFINITY, |m| m + BONNET_MIDDLE_CLEARANCE);
    let eye = Vec3::new(place.eye.x, (height + BONNET_CLEARANCE).max(middle), z);
    let front = profile.iter().position(|&h| h >= height - BONNET_FRONT_DROP).unwrap_or(0);
    let look = place.target - place.eye;
    let mut pitch = look.y.atan2(look.z.hypot(look.x));
    if front < rear {
        let edge = (profile[front] - eye.y).atan2(eye.z - z_at(front));
        pitch = pitch.min(edge + BONNET_EDGE_BELOW.to_radians()).max(pitch - BONNET_TILT_MAX.to_radians());
    }
    let flat = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
    let target = eye + (flat * pitch.cos() + Vec3::Y * pitch.sin()) * look.length();
    Mount { eye, target, ..place }
}

/// How far the cockpit eye moves back from `Cam_internal`'s eye: [`COCKPIT_BACK`], or further to [`WHEEL_BEHIND`] the
/// steering `wheel` (at most [`COCKPIT_BACK_MAX`], on whole [`COCKPIT_STEP`]s), or the most less by [`COCKPIT_STEP`]s
/// from which no `interior` surface the view shows lies nearer than [`COCKPIT_CLEAR`] along [`COCKPIT_LOOK`], nor
/// nearer than [`FRAME_CLEAR`] across the default frame with the view tilted to the hub when `narrow` ([`frame_clear`],
/// [`hub_in_view`]) (0 when every step is blocked). Ours: the game uses `Pos` as is. A surface counts as shown unless
/// it is alpha blended or a culled back face, so the eye may pass a headrest edge or a cage tube that it then sees from
/// behind. The retail cars move the whole 0.5 m; the RSF GR Yaris's eye is 0.07 m in front of its headrest's top and
/// 0.2 m in front of its seat shell, and moves 0.1 m; the RSF Audi quattro A2's moves 0.35 m, where its seat's bolsters
/// stay under the frame's bottom edge; the RSF Fiesta Rally3's moves 0.85 m, from over its dash to behind its wheel.
pub fn cockpit_back(internal: Mount, interior: &[Mesh], wheel: Option<Vec3>, narrow: bool) -> f32 {
    let shown = shown(interior);
    let behind = wheel.map_or(0.0, |w| w.z + WHEEL_BEHIND - internal.eye.z).min(COCKPIT_BACK_MAX);
    let start = if behind > COCKPIT_BACK { (behind / COCKPIT_STEP).ceil() * COCKPIT_STEP } else { COCKPIT_BACK };
    let steps = (start / COCKPIT_STEP).round() as usize;
    let aimed = |m: Mount| wheel.filter(|_| narrow).map_or(m, |wheel| hub_in_view(m, wheel, &shown));
    (0..=steps)
        .map(|k| if k == steps { 0.0 } else { start - k as f32 * COCKPIT_STEP })
        .find(|&back| {
            let m = internal.moved_back(back);
            clear(&shown, m, COCKPIT_CLEAR) && frame_clear(&shown, aimed(m), FRAME_CLEAR)
        })
        .unwrap_or(0.0)
}

/// The cockpit `mount` looking down far enough that the steering `wheel`'s hub lies at most [`HUB_BELOW`] under the
/// bottom of the default image, by at most [`HUB_TILT_MAX`] more, in [`HUB_TILT_STEP`]s, and not so far that a
/// `shown` surface lies nearer than [`HUB_CLEAR`] along [`COCKPIT_LOOK`] (the RSF Polo GTI R5's eye
/// is just above a panel there). Ours.
fn hub_in_view(mount: Mount, wheel: Vec3, shown: &[Tri]) -> Mount {
    let down = |v: Vec3| (-v.y).atan2(Vec3::new(v.x, 0.0, v.z).length()).to_degrees();
    let pitch = down(mount.target - mount.eye);
    let tilt = (down(wheel - mount.eye) - HALF_VERTICAL - HUB_BELOW - pitch).min(HUB_TILT_MAX);
    if tilt <= 0.0 {
        return mount;
    }
    let steps = (tilt / HUB_TILT_STEP).ceil() as usize;
    (0..=steps)
        .rev()
        .map(|k| mount.looking_down(pitch + tilt * k as f32 / steps as f32))
        .find(|&tilted| clear(shown, tilted, HUB_CLEAR))
        .unwrap_or(mount)
}

/// A triangle a view shows, with whether it is culled at its back.
type Tri = ([Vec3; 3], bool);

/// The triangles of `interior` a view shows: not alpha blended, with whether they are culled at their back.
fn shown(interior: &[Mesh]) -> Vec<Tri> {
    interior
        .iter()
        .flat_map(|mesh| {
            mesh.draws.iter().filter(|draw| !draw.state.blend).flat_map(move |draw| {
                mesh.indices[draw.indices.clone()].chunks_exact(3).map(move |t| {
                    let at = |k: usize| Vec3::from(mesh.vertices[t[k] as usize].position);
                    ([at(0), at(1), at(2)], draw.state.cull_back)
                })
            })
        })
        .collect()
}

/// Whether none of the `shown` triangles lies nearer than `distance` to `mount`'s eye along [`COCKPIT_LOOK`].
fn clear(shown: &[Tri], mount: Mount, distance: f32) -> bool {
    let shown = near(shown, mount.eye, distance);
    let (ahead, right, up) = axes(mount);
    COCKPIT_LOOK.iter().all(|&x| {
        COCKPIT_LOOK_UP.iter().all(|&y| {
            let dir = (ahead + right * x + up * y).normalize();
            !shown.iter().any(|&(t, cull_back)| {
                drawn(t, cull_back, dir) && ray_hit(mount.eye, dir, t).is_some_and(|d| d < distance)
            })
        })
    })
}

/// Whether none of the `shown` triangles lies between the near plane and `distance` from `mount`'s eye, or from the
/// eye [`SHAKE_DIP`] lower, along [`FRAME_COLUMNS`] by [`FRAME_ROWS`] lines across the default frame
/// ([`coords::FOV_DEGREES`] across at 16:9, edges included).
fn frame_clear(shown: &[Tri], mount: Mount, distance: f32) -> bool {
    let shown = near(shown, mount.eye, distance + SHAKE_DIP);
    let (ahead, right, up) = axes(mount);
    let eyes = [mount.eye, mount.eye - Vec3::Y * SHAKE_DIP];
    let across = (coords::FOV_DEGREES.to_radians() * 0.5).tan();
    let down = HALF_VERTICAL.to_radians().tan();
    let spread = |i: usize, n: usize| 2.0 * i as f32 / (n - 1) as f32 - 1.0;
    (0..FRAME_COLUMNS).all(|i| {
        (0..FRAME_ROWS).all(|j| {
            let dir = ahead + right * (across * spread(i, FRAME_COLUMNS)) + up * (down * spread(j, FRAME_ROWS));
            let dir = dir.normalize();
            // The near plane clips what lies nearer along the view axis.
            let shows = |d: f32| d < distance && d * dir.dot(ahead) > mount.near;
            !shown.iter().any(|&(t, cull_back)| {
                drawn(t, cull_back, dir) && eyes.iter().any(|&eye| ray_hit(eye, dir, t).is_some_and(shows))
            })
        })
    })
}

/// The `shown` triangles whose boxes come within `reach` of `eye` (and 1 cm for the rounding): no other lies nearer
/// than `reach` along any line from it. The cockpit's lines tested against every interior triangle took 0.4 s a car.
fn near(shown: &[Tri], eye: Vec3, reach: f32) -> Vec<Tri> {
    let reach = reach + 0.01;
    let within = |t: &[Vec3; 3]| {
        let (low, high) = (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]));
        (low - eye).max(eye - high).max(Vec3::ZERO).length() <= reach
    };
    shown.iter().filter(|(t, _)| within(t)).copied().collect()
}

/// `mount`'s ahead, right and up, unit.
fn axes(mount: Mount) -> (Vec3, Vec3, Vec3) {
    let ahead = (mount.target - mount.eye).normalize();
    let right = ahead.cross(Vec3::Y).normalize();
    (ahead, right, right.cross(ahead))
}

/// Whether the triangle `t` is drawn seen along `dir`: drawn both sides, or its front faces the viewer (its normal
/// `e1 x e2` points at it, `coords::FRONT_FACE`).
fn drawn(t: [Vec3; 3], cull_back: bool, dir: Vec3) -> bool {
    !cull_back || (t[1] - t[0]).cross(t[2] - t[0]).dot(dir) < 0.0
}

/// The distance along `dir` (unit) from `from` to the triangle `t`, either face (Moller-Trumbore).
pub fn ray_hit(from: Vec3, dir: Vec3, t: [Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = from - t[0];
    let u = s.dot(p) / det;
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    let d = e2.dot(q) / det;
    ((0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && d > 0.0).then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::ini;

    #[test]
    fn the_start_orbit_turns_the_chase_until_the_go() {
        use rbr_recording::camera::{MODE_CHASE, MODE_START_ORBIT};
        let orbit = Some(Camera { mode: MODE_START_ORBIT, angle: 3.0 });
        assert_eq!(start_orbit(orbit), Some(3.0));
        assert_eq!(start_orbit(Some(Camera { mode: MODE_CHASE, angle: 0.0 })), None);
        assert_eq!(start_orbit(None), None);
        // The orbit comes first, then the helicam; the helicam blocks C.
        assert_eq!(Shown::of(CarView::Cockpit, orbit, true, true), Shown::StartOrbit(3.0));
        assert_eq!(Shown::of(CarView::Cockpit, orbit, true, false).car_view(), CarView::Chase);
        let heli = Shown::of(CarView::Cockpit, None, true, true);
        assert!(heli == Shown::Helicam && !heli.may_cycle() && heli.swing().is_none());
        let flip = Shown::of(CarView::Cockpit, None, false, true);
        assert!(flip == Shown::FlipCam && !flip.may_cycle() && flip.car_view() == CarView::Chase);
        assert_eq!(Shown::of(CarView::Bumper, None, false, false), Shown::Player(CarView::Bumper));
    }

    #[test]
    fn views_cycle_and_parse_by_name() {
        let mut view = CarView::default();
        let names: Vec<&str> = (0..5)
            .map(|_| {
                let name = view.name();
                view = view.next();
                name
            })
            .collect();
        assert_eq!(names, ["chase", "cockpit", "bonnet", "bumper", "chase"]);
        assert_eq!(CarView::parse("Cockpit"), Some(CarView::Cockpit));
        assert_eq!(CarView::parse("bonnet2"), None);
        assert!(CarView::Cockpit.interior() && CarView::Cockpit.shakes());
        assert!(!CarView::Bonnet.interior() && !CarView::Bumper.interior() && !CarView::Chase.interior());
        assert!(CarView::ALL.iter().enumerate().all(|(i, v)| v.index() == i));
    }

    #[test]
    fn mounts_read_the_camera_sections() {
        // COROLLA's sections.
        let sections = ini::parse(
            "[Cam_bonnet]\r\nFOV = 1.308997\r\nNear = 0.083100\r\nPos = 0.000000 0.831452 -2.846988\r\n\
             Target = 0.000000 0.681264 -5.624203\r\nUp = 0.000000 1.000000 0.000000\r\n\
             [Cam_internal]\r\nFOV = 1.308997\r\nNear = 0.193100\r\nPos = 0.329818 1.022179 -1.225576\r\n\
             Target = 0.329818 0.895248 -3.788919\r\nUp = 0.000000 1.000000 0.000000\r\n\
             [Cam_bonnet2]\r\nFOV = 1.308997\r\nNear = 0.150000\r\nPos = 0.000000 1.009285 -1.234858\r\n\
             Target = 0.000000 1.009285 -5.020657\r\nUp = 0.000000 1.000000 0.000000\r\n",
        )
        .unwrap();
        let lo = Vec3::new(-0.9, -0.3, -3.4);
        let hi = Vec3::new(0.9, 1.1, 0.9);
        let mounts = Mounts::from_sections(&sections, lo, hi, &[], &[], None).unwrap();
        assert!((mounts.cockpit.eye - Vec3::new(0.329818, 1.022179, -1.225576 + COCKPIT_BACK)).length() < 1e-5);
        assert_eq!(mounts.cockpit.near, 0.1931);
        assert_eq!(mounts.bonnet.near, 0.0831);
        assert!((mounts.bonnet.eye - Vec3::new(0.0, 0.831452, -2.846988)).length() < 1e-5, "nothing under it");
        assert!((mounts.bumper.eye - Vec3::new(0.0, 0.15, -3.45)).length() < 1e-5);
        assert!(mounts.of(CarView::Chase).is_none());
        // The shake's body z (up) is the model's y.
        let shaken = mounts.cockpit.shaken(Vec3::new(0.01, 0.02, 0.03));
        assert!((shaken.eye - mounts.cockpit.eye - Vec3::new(0.01, 0.03, 0.02)).length() < 1e-6);
        // A nudge right, up and forward is model -x, +y and -z, with the target.
        let mut nudged = mounts;
        nudged.nudges[CarView::Cockpit.index()] = Nudge { shift: Vec3::new(0.1, 0.2, 0.3), tilt: 0.0 };
        let m = nudged.of(CarView::Cockpit).unwrap();
        assert!((m.eye - mounts.cockpit.eye - Vec3::new(-0.1, 0.2, -0.3)).length() < 1e-6);
        assert!((m.target - m.eye - (mounts.cockpit.target - mounts.cockpit.eye)).length() < 1e-5);
        // A tilt turns the view up about the eye after the cockpit's own pitch, the target as far.
        let pitch =
            |m: Mount| (m.target - m.eye).y.atan2(Vec3::new(m.target.x - m.eye.x, 0.0, m.target.z - m.eye.z).length());
        nudged.nudges[CarView::Cockpit.index()] = Nudge { shift: Vec3::ZERO, tilt: -2.0 };
        let tilted = nudged.of(CarView::Cockpit).unwrap();
        assert_eq!(tilted.eye, mounts.cockpit.eye);
        assert!((pitch(tilted) - pitch(mounts.cockpit) + 2.0f32.to_radians()).abs() < 1e-5);
        assert!(
            ((tilted.target - tilted.eye).length() - (mounts.cockpit.target - mounts.cockpit.eye).length()).abs()
                < 1e-5
        );
        assert!(Mounts::from_sections(&sections[..1], lo, hi, &[], &[], None).is_err());
        // Without a usable Cam_bonnet, the bonnet eye is on the centre line ahead of the internal camera.
        let emptied = ini::parse("[Cam_bonnet]\r\nPos =\r\n").unwrap().into_iter().chain(sections[1..].to_vec());
        let mounts = Mounts::from_sections(&emptied.collect::<Vec<_>>(), lo, hi, &[], &[], None).unwrap();
        let fallback = Vec3::new(0.0, 1.022179 - BONNET_BELOW_INTERNAL, -1.225576 - BONNET_AHEAD_OF_INTERNAL);
        assert!((mounts.bonnet.eye - fallback).length() < 1e-5);
        assert!((mounts.bonnet.target - mounts.bonnet.eye - BONNET_LOOK).length() < 1e-5);
        // A copy of the internal camera with the model's own kept as the backup: the backup.
        let copied = ini::parse(
            "[Cam_bonnet]\r\nNear = 0.1931\r\nPos = 0.329818 1.022179 -1.225576\r\nTarget = 0.329818 0.895248 -3.788919\r\n\
             Backup_Near = 0.0831\r\nBackup_Pos = 0 0.82 -3.0\r\nBackup_Target = 0 0.7 -5.8\r\n",
        )
        .unwrap()
        .into_iter()
        .chain(sections[1..].to_vec());
        let mounts = Mounts::from_sections(&copied.collect::<Vec<_>>(), lo, hi, &[], &[], None).unwrap();
        assert_eq!((mounts.bonnet.eye, mounts.bonnet.near), (Vec3::new(0.0, 0.82, -3.0), 0.0831));
    }

    /// A two-triangle mesh through four corners.
    fn quad(corners: [Vec3; 4]) -> Mesh {
        use crate::stage::{Address, Anchor, Draw, DrawState, Layer, Lod, Vertex};
        use bytemuck::Zeroable;
        Mesh {
            layer: Layer::Solid,
            anchor: Anchor::World,
            bounds: None,
            cell: None,
            vertices: corners.map(|p| Vertex { position: p.into(), ..Vertex::zeroed() }).to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            draws: vec![Draw {
                indices: 0..6,
                state: DrawState {
                    textures: [None, None],
                    shadow: None,
                    address: [Address::Wrap; 2],
                    second_texture: false,
                    alpha_ref: None,
                    mip_bias: false,
                    cull_back: false,
                    blend: false,
                    additive: false,
                    depth_test: true,
                    depth_write: true,
                    shade: None,
                    specular: None,
                },
                lod: Lod::Always,
            }],
        }
    }

    #[test]
    fn the_bonnet_eye_sits_just_above_the_bonnet_rear() {
        let place =
            Mount { eye: Vec3::new(0.0, 0.8, -3.0), target: Vec3::new(0.0, 0.7, -6.0), up: Vec3::Y, near: 0.08 };
        // A bonnet rising gently from 0.7 m at z -3.2 to 0.8 m at -2.0, then a windscreen up to 1.3 m at -1.2.
        let across = |(z0, y0): (f32, f32), (z1, y1): (f32, f32)| {
            quad([Vec3::new(-1.0, y0, z0), Vec3::new(1.0, y0, z0), Vec3::new(1.0, y1, z1), Vec3::new(-1.0, y1, z1)])
        };
        let car = [across((-3.2, 0.7), (-2.0, 0.8)), across((-2.0, 0.8), (-1.2, 1.3))];
        let eye = bonnet_mount(place, &car);
        assert!((-2.25..=-2.0).contains(&eye.eye.z), "{eye:?}");
        assert!((eye.eye.y - (0.78 + BONNET_CLEARANCE)).abs() < 0.03, "{eye:?}");
        // It looks down far enough to put the front edge (the walk's first point) BONNET_EDGE_BELOW below the middle,
        // or along `place` when that is lower.
        let look = eye.target - eye.eye;
        let pitch = |v: Vec3| (v.y / v.length()).asin();
        let edge = pitch(Vec3::new(0.0, 0.7 + 0.1 / 1.2 * 0.2, -3.0) - eye.eye);
        let wanted = pitch(place.target - place.eye).min(edge + BONNET_EDGE_BELOW.to_radians());
        assert!((pitch(look) - wanted).abs() < 0.01, "{} {}", pitch(look).to_degrees(), wanted.to_degrees());
        assert_eq!(look.x, 0.0);
        // Nothing under it keeps it.
        assert_eq!(bonnet_mount(place, &[]), place);
    }

    #[test]
    fn the_cockpit_view_looks_down_at_least_cockpit_pitch() {
        let mount = |target: Vec3| Mount { eye: Vec3::new(0.3, 1.0, -1.0), target, up: Vec3::Y, near: 0.1 };
        // Down 4.6 degrees: kept.
        let down = mount(Vec3::new(0.3, 0.8, -3.5));
        assert_eq!(down.looking_down(COCKPIT_PITCH), down);
        // Up: turned down to COCKPIT_PITCH about the eye, the target as far.
        let up = mount(Vec3::new(0.3, 1.1, -3.5));
        let turned = up.looking_down(COCKPIT_PITCH);
        let look = turned.target - turned.eye;
        assert_eq!(turned.eye, up.eye);
        assert!((look.length() - (up.target - up.eye).length()).abs() < 1e-5);
        assert!(((-look.y / look.length()).asin().to_degrees() - COCKPIT_PITCH).abs() < 1e-4);
        assert!(look.x.abs() < 1e-6 && look.z < 0.0);
    }

    #[test]
    fn the_cockpit_eye_stops_where_the_seat_would_block_the_view() {
        // A seat back: a 1 m square across the car, `z` behind the eye at (0, 1, -1). Its front face looks back (+z)
        // or, `towards_eye`, forward; drawn both sides or culled at its back.
        let seat = |z: f32, towards_eye: bool, cull_back: bool| {
            let mut corners = [(-0.5, 0.5), (0.5, 0.5), (0.5, 1.5), (-0.5, 1.5)];
            if towards_eye {
                corners.reverse();
            }
            let mut mesh = quad(corners.map(|(x, y)| Vec3::new(x, y, -1.0 + z)));
            mesh.draws[0].state.cull_back = cull_back;
            mesh
        };
        let internal =
            Mount { eye: Vec3::new(0.0, 1.0, -1.0), target: Vec3::new(0.0, 1.0, -3.0), up: Vec3::Y, near: 0.1 };
        assert_eq!(cockpit_back(internal, &[], None, false), COCKPIT_BACK);
        for cull_back in [false, true] {
            // Not passed.
            assert_eq!(cockpit_back(internal, &[seat(0.9, false, cull_back)], None, false), COCKPIT_BACK);
            // Passed, it is nearer than COCKPIT_CLEAR ahead from 0.5 to 0.35 m back.
            assert!((cockpit_back(internal, &[seat(0.32, false, cull_back)], None, false) - 0.3).abs() < 1e-6);
            // A surface in front of the eye stays further than COCKPIT_CLEAR.
            assert_eq!(cockpit_back(internal, &[seat(-0.3, false, cull_back)], None, false), COCKPIT_BACK);
        }
        // Passed, a surface culled at its back that faced the eye is not drawn; drawn both sides, it blocks.
        assert_eq!(cockpit_back(internal, &[seat(0.42, true, true)], None, false), COCKPIT_BACK);
        assert!((cockpit_back(internal, &[seat(0.42, true, false)], None, false) - 0.4).abs() < 1e-6);
        // A wheel further than WHEEL_BEHIND ahead of the moved eye changes nothing; one nearer moves the eye on to it,
        // on whole steps and at most COCKPIT_BACK_MAX, and the seat still stops it.
        assert_eq!(cockpit_back(internal, &[], Some(Vec3::new(0.0, 0.7, -1.2)), false), COCKPIT_BACK);
        assert!((cockpit_back(internal, &[], Some(Vec3::new(0.0, 0.7, -0.9)), false) - 0.75).abs() < 1e-6);
        assert!((cockpit_back(internal, &[], Some(Vec3::new(0.0, 0.7, 0.0)), false) - COCKPIT_BACK_MAX).abs() < 1e-6);
        let wheel = Some(Vec3::new(0.0, 0.7, -0.9));
        assert!((cockpit_back(internal, &[seat(0.62, false, false)], wheel, false) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn the_cockpit_eye_comes_forward_until_no_seat_edge_enters_the_frame() {
        let internal =
            Mount { eye: Vec3::new(0.0, 1.0, -1.0), target: Vec3::new(0.0, 1.0, -3.0), up: Vec3::Y, near: 0.1 };
        // A seat cushion's front 0.1 m under the eye, 0.25 to 0.4 m ahead of it 0.5 m back: under the frame's bottom
        // edge from there (0.39 m along it), in it from the eye SHAKE_DIP lower (0.28 m). The eye comes forward until
        // that edge passes over the cushion's front (0.37 m back).
        let seat = quad([(-0.5, -0.9), (0.5, -0.9), (0.5, -0.75), (-0.5, -0.75)].map(|(x, z)| Vec3::new(x, 0.9, z)));
        assert!((cockpit_back(internal, std::slice::from_ref(&seat), None, false) - 0.35).abs() < 1e-6);
        // Under the near plane it is not drawn.
        assert_eq!(cockpit_back(Mount { near: 0.3, ..internal }, &[seat], None, false), COCKPIT_BACK);
        // A dash 0.7 m ahead fills the bottom of the frame and changes nothing.
        let dash = quad([(-0.5, 0.5), (0.5, 0.5), (0.5, 1.0), (-0.5, 1.0)].map(|(x, y)| Vec3::new(x, y, -1.2)));
        assert_eq!(cockpit_back(internal, &[dash], None, false), COCKPIT_BACK);
    }

    #[test]
    fn the_narrow_cockpit_view_looks_down_to_the_wheel_hub() {
        let mount = Mount { eye: Vec3::new(0.0, 1.0, -1.0), target: Vec3::new(0.0, 1.0, -3.0), up: Vec3::Y, near: 0.1 };
        let down = |m: Mount| {
            let look = m.target - m.eye;
            (-look.y).atan2(look.z.hypot(look.x)).to_degrees()
        };
        // 20 degrees down: tilted to put it HUB_BELOW under the image's bottom.
        let hub = Vec3::new(0.0, 1.0 - 20f32.to_radians().tan(), -2.0);
        assert!((down(hub_in_view(mount, hub, &[])) - (20.0 - HALF_VERTICAL - HUB_BELOW)).abs() < 1e-3);
        // Already in view: kept; far below: at most HUB_TILT_MAX.
        assert_eq!(hub_in_view(mount, Vec3::new(0.0, 0.9, -2.0), &[]), mount);
        let low = Vec3::new(0.0, 1.0 - 40f32.to_radians().tan(), -2.0);
        assert!((down(hub_in_view(mount, low, &[])) - HUB_TILT_MAX).abs() < 1e-3);
        // A panel 0.2 m ahead under the eye stops the tilt where the lowest look line would meet it.
        let panel = quad([(-1.0, -1.2), (1.0, -1.2), (1.0, -1.0), (-1.0, -1.0)].map(|(x, z)| Vec3::new(x, 0.97, z)));
        let tilted = down(hub_in_view(mount, low, &shown(&[panel])));
        assert!(tilted > 0.0 && tilted < HUB_TILT_MAX, "{tilted}");
    }
}
