//! The car setup the player edits: every value the game's setup pages change, their ranges and steps, and the setup
//! files the game reads and writes.
//!
//! The pages (rbr-recomp `notes/progress/systems.md`, section 4) build one slider per value (`Vf38` of `CTyresPage` 0x554b30,
//! `CBrakesPage` 0x554de0, `CGeometryPageFront` 0x554f80, `CGeometryPageRear` 0x555750, `CRollbarPage` 0x555b60,
//! `CGearsPage` 0x556120, `CDifferentialTorquePage` 0x550750, `CSpringsPage` 0x550990, `CDampersPage` 0x550fd0,
//! `CDifferentialMapPage` 0x551610). Each slider holds the setup's tweak and three tweaks of the car's options file
//! (`r_<surface>.lsp`): the minimum, the maximum and the step (`E55_MakeSliderA` 0x550130). A press adds or takes one
//! step and then clamps, or wraps where the page asks for it (tyre and brake pressures): [`Range::up`] and
//! [`Range::down`] (`CTweakSliderMenuItemReal` 0x54ef80/0x54ee80, `...Unsigned` 0x54f140/0x54f050). The top mount's
//! slider runs from 0 to the options' `nTopMountPositions` by 1 (`E55_MakeSliderB` 0x5501f0), the drop gear's over
//! `DropGearOptionsMin..Max` by 1 (`E56_MakeDropGear` 0x555c70).
//!
//! The defaults are the car's `setups/d_<surface>.lsp`. A saved setup is the game's own format: the setup's tweak tree as
//! `.lsp` text (`E16_BuildSetup` 0x48d850 writes it, `CCarSaveFileUI::SaveSetup` 0x546510 reads `%sSetups/<name>` back
//! with the loader the default file takes, `CCarSaveFileUI::Scan` 0x546a30 lists `%sSetups/*.lsp`). The loader layers a
//! file over the setup it has, so [`Setup::to_text`] writes the values the pages edit and the physics boot reads it
//! after the default file. The text layout is ours (the game's printer, `CBase::GetValueText` 0x52f590, puts every
//! tweak of the tree; its spacing is not matched); the game's loader takes it.

use rbr_formats::lsp::{self, Node};

/// The wheels, in the setup's key order.
pub const WHEELS: [&str; 4] = ["LF", "RF", "LB", "RB"];
/// The control unit's four differential maps, by key prefix.
pub const DIFF_MAPS: [&str; 4] = ["Center", "LFCenter", "Front", "Rear"];
/// Points per map curve (`_00` to `_10`).
pub const MAP_POINTS: usize = 11;

/// The car folders of the retail cars (`[CarNN]` order), the exe's table at 0x7c18d4.
pub const CAR_FOLDERS: [&str; 8] = [
    "Physics/c_xsara/",
    "Physics/h_accent/",
    "Physics/mg_zr/",
    "Physics/m_lancer/",
    "Physics/p_206/",
    "Physics/s_i2003/",
    "Physics/t_coroll/",
    "Physics/s_i2000/",
];

/// One wheel's values: its tyre, spring, damper and geometry pages.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Corner {
    pub tyre_pressure: f32,
    pub spring_length: f32,
    pub spring_stiffness: f32,
    pub helper_spring_length: f32,
    pub helper_spring_stiffness: f32,
    pub bump: f32,
    pub rebound: f32,
    pub fast_bump: f32,
    pub fast_bump_threshold: f32,
    pub top_mount_slot: f32,
    pub steering_rod_length: f32,
    pub platform_height: f32,
    pub axis_inclination: f32,
}

/// One differential map of the control unit: lock against throttle and brake, and the lock factor against speed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DiffMap {
    pub throttle: [f32; MAP_POINTS],
    pub brake: [f32; MAP_POINTS],
    pub speed: [f32; MAP_POINTS],
    pub speed_factor: [f32; MAP_POINTS],
}

/// Every value the setup pages edit.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Setup {
    pub steering_lock: f32,
    pub front_roll_bar: f32,
    pub rear_roll_bar: f32,
    pub front_brake_pressure: f32,
    pub rear_brake_pressure: f32,
    pub handbrake_release: f32,
    pub center_diff_torque: f32,
    pub front_diff_torque: f32,
    pub rear_diff_torque: f32,
    pub left_foot_brake_threshold: f32,
    pub drop_gear: u32,
    pub diff_maps: [DiffMap; 4],
    pub corners: [Corner; 4],
}

/// The launcher's grouping of the values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Tyres,
    Springs,
    Dampers,
    Geometry,
    RollBars,
    Brakes,
    Differentials,
    Gears,
}

impl Area {
    pub const ALL: [Area; 8] = [
        Area::Tyres,
        Area::Springs,
        Area::Dampers,
        Area::Geometry,
        Area::RollBars,
        Area::Brakes,
        Area::Differentials,
        Area::Gears,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Area::Tyres => "Tyres",
            Area::Springs => "Springs",
            Area::Dampers => "Dampers",
            Area::Geometry => "Geometry",
            Area::RollBars => "Anti-roll bars and steering",
            Area::Brakes => "Brakes",
            Area::Differentials => "Differentials",
            Area::Gears => "Gears",
        }
    }
}

/// How a value is shown: the unit and the factor from the file's SI value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unit {
    pub suffix: &'static str,
    pub scale: f32,
    pub decimals: usize,
}

const BAR: Unit = Unit { suffix: "bar", scale: 1e-5, decimals: 2 };
const MM: Unit = Unit { suffix: "mm", scale: 1e3, decimals: 2 };
const N_MM: Unit = Unit { suffix: "N/mm", scale: 1e-3, decimals: 0 };
const NS_M: Unit = Unit { suffix: "Ns/m", scale: 1.0, decimals: 0 };
const M_S: Unit = Unit { suffix: "m/s", scale: 1.0, decimals: 2 };
const DEG: Unit = Unit { suffix: "°", scale: 180.0 / std::f32::consts::PI, decimals: 2 };
const NM: Unit = Unit { suffix: "Nm", scale: 1.0, decimals: 0 };
const PERCENT: Unit = Unit { suffix: "%", scale: 100.0, decimals: 0 };
const KMH: Unit = Unit { suffix: "km/h", scale: 3.6, decimals: 1 };
const ROLL_BAR: Unit = Unit { suffix: "N/m", scale: 1.0, decimals: 0 };
const SLOT: Unit = Unit { suffix: "", scale: 1.0, decimals: 0 };

impl Unit {
    pub fn show(self, value: f32) -> String {
        format!("{:.*} {}", self.decimals, value * self.scale, self.suffix).trim_end().to_owned()
    }
}

/// Where a value's range is in the options file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bounds {
    /// `<prefix>Min`, `<prefix>Max`, `<prefix>Step`.
    Triple(&'static str),
    /// 0 to `nTopMountPositions` by 1.
    TopMount,
    /// `DropGearOptionsMin` to `DropGearOptionsMax` by 1.
    DropGear,
}

/// One value: its page, its key in the setup file and its range's keys in the options file.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub area: Area,
    pub label: &'static str,
    /// The wheel (index into [`WHEELS`]) or differential map (into [`DIFF_MAPS`]) it belongs to.
    pub index: Option<usize>,
    pub group: String,
    pub key: String,
    pub options: String,
    pub bounds: Bounds,
    /// The slider wraps past its ends (the pages' `e` argument).
    pub wrap: bool,
    pub unit: Unit,
}

/// A value of the setup, as its tweak holds it.
pub enum Slot<'a> {
    Real(&'a mut f32),
    Unsigned(&'a mut u32),
}

/// A slider's range: the options' minimum, maximum and step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub wrap: bool,
}

impl Range {
    /// One step up: `CTweakSliderMenuItemReal::Vf38` (0x54ef80).
    pub fn up(self, value: f32) -> f32 {
        let value = value + self.step;
        if value > self.max {
            if self.wrap {
                self.min
            } else {
                self.max
            }
        } else {
            value
        }
    }

    /// One step down: `CTweakSliderMenuItemReal::Vf34` (0x54ee80). Past the maximum it takes the minimum unless it
    /// wraps, as the original does.
    pub fn down(self, value: f32) -> f32 {
        let mut value = value - self.step;
        if self.min > value {
            value = if self.wrap { self.max } else { self.min };
        }
        if value > self.max {
            value = if self.wrap { self.max } else { self.min };
        }
        value
    }

    /// The value the steps from `value` reach closest to `target` without wrapping (ours: a slider drag as presses).
    pub fn toward(self, mut value: f32, target: f32) -> f32 {
        if self.step <= 0.0 {
            return value;
        }
        loop {
            let next = if target > value { self.up(value) } else { self.down(value) };
            if (next - target).abs() >= (value - target).abs() {
                return value;
            }
            value = next;
        }
    }
}

fn triple(area: Area, label: &'static str, group: &str, key: &str, options: &str, prefix: &'static str) -> Spec {
    Spec {
        area,
        label,
        index: None,
        group: group.to_owned(),
        key: key.to_owned(),
        options: options.to_owned(),
        bounds: Bounds::Triple(prefix),
        wrap: false,
        unit: SLOT,
    }
}

impl Spec {
    fn at(self, index: usize) -> Spec {
        Spec { index: Some(index), ..self }
    }

    fn unit(self, unit: Unit) -> Spec {
        Spec { unit, ..self }
    }

    fn wrapping(self) -> Spec {
        Spec { wrap: true, ..self }
    }
}

impl Setup {
    /// Every value with its spec, in the setup file's group order.
    pub fn visit(&mut self, f: &mut dyn FnMut(Spec, Slot<'_>)) {
        use Area::*;
        let car = |label, key, prefix| triple(RollBars, label, "Car", key, "CarOptions", prefix);
        f(
            car("Steering lock", "MaxSteeringLock", "MaxSteeringLockRange").unit(DEG),
            Slot::Real(&mut self.steering_lock),
        );
        f(
            car("Front anti-roll bar", "FrontRollBarStiffness", "FrontRollBarStiffness").unit(ROLL_BAR),
            Slot::Real(&mut self.front_roll_bar),
        );
        f(
            car("Rear anti-roll bar", "RearRollBarStiffness", "RearRollBarStiffness").unit(ROLL_BAR),
            Slot::Real(&mut self.rear_roll_bar),
        );

        let drive = |area, label, key, prefix| triple(area, label, "Drive", key, "DriveOptions", prefix);
        f(
            drive(Differentials, "Centre diff torque", "CenterDiffMaxTorque", "CenterDiffTorque").unit(NM),
            Slot::Real(&mut self.center_diff_torque),
        );
        f(
            drive(Differentials, "Front diff torque", "FrontDiffMaxTorque", "FrontDiffTorque").unit(NM),
            Slot::Real(&mut self.front_diff_torque),
        );
        f(
            drive(Differentials, "Rear diff torque", "RearDiffMaxTorque", "RearDiffTorque").unit(NM),
            Slot::Real(&mut self.rear_diff_torque),
        );
        f(
            drive(Brakes, "Front brake pressure", "MaxBrakePressureFront", "FrontBrakePressure").unit(BAR).wrapping(),
            Slot::Real(&mut self.front_brake_pressure),
        );
        f(
            drive(Brakes, "Rear brake pressure", "MaxBrakePressureRear", "RearBrakePressure").unit(BAR).wrapping(),
            Slot::Real(&mut self.rear_brake_pressure),
        );
        let drop = Spec { bounds: Bounds::DropGear, ..drive(Gears, "Drop gear", "DropGearId", "") };
        f(drop, Slot::Unsigned(&mut self.drop_gear));

        let vcu =
            |area, label, key: &str, prefix| triple(area, label, "VehicleControlUnit", key, "ControlOptions", prefix);
        f(
            vcu(
                Brakes,
                "Handbrake centre-diff release",
                "CenterDiffHandbrakeRelease",
                "CenterDiffHandbrakeReleaseRange",
            )
            .unit(PERCENT),
            Slot::Real(&mut self.handbrake_release),
        );
        f(
            vcu(Differentials, "Left-foot brake threshold", "LeftFootBrakeThreshold", "LeftFootBrakeThresholdRange")
                .unit(PERCENT),
            Slot::Real(&mut self.left_foot_brake_threshold),
        );
        for (m, (map, prefix)) in self.diff_maps.iter_mut().zip(DIFF_MAPS).enumerate() {
            for (curve, key, range, unit) in [
                (&mut map.throttle, format!("{prefix}DiffThrottle"), "DiffMapLockRange", PERCENT),
                (&mut map.brake, format!("{prefix}DiffBrake"), "DiffMapLockRange", PERCENT),
                (&mut map.speed, format!("{prefix}SpeedMapVelocity"), "DiffMapSpeedRange", KMH),
                (&mut map.speed_factor, format!("{prefix}SpeedMapFactor"), "DiffMapLockRange", PERCENT),
            ] {
                for (n, value) in curve.iter_mut().enumerate() {
                    let spec = vcu(Differentials, "Diff map", &format!("{key}_{n:02}"), range).unit(unit).at(m);
                    f(spec, Slot::Real(value));
                }
            }
        }

        for (w, (corner, wheel)) in self.corners.iter_mut().zip(WHEELS).enumerate() {
            let (group, options) = (format!("Wheel{wheel}"), format!("WheelOptions{wheel}"));
            let geometry = |label, key, prefix| triple(Geometry, label, &group, key, &options, prefix).at(w);
            let top = Spec { bounds: Bounds::TopMount, ..geometry("Top mount", "TopMountSlot", "") };
            f(top, Slot::Real(&mut corner.top_mount_slot));
            f(
                geometry("Toe (steering rod)", "SteeringRodLength", "SteeringRod").unit(MM),
                Slot::Real(&mut corner.steering_rod_length),
            );
            f(
                geometry("Ride height (platform)", "StrutPlatformHeight", "PlatformHeight").unit(MM),
                Slot::Real(&mut corner.platform_height),
            );
            f(
                geometry("Camber (axis inclination)", "WheelAxisInclination", "WheelAxisIncl").unit(DEG),
                Slot::Real(&mut corner.axis_inclination),
            );
        }
        for (w, (corner, wheel)) in self.corners.iter_mut().zip(WHEELS).enumerate() {
            let (group, options) = (format!("SpringDamper{wheel}"), format!("SpringDamperOptions{wheel}"));
            let spring = |area, label, key, prefix| triple(area, label, &group, key, &options, prefix).at(w);
            f(
                spring(Springs, "Spring length", "SpringLength", "SpringLength").unit(MM),
                Slot::Real(&mut corner.spring_length),
            );
            f(
                spring(Springs, "Spring rate", "SpringStiffness", "SpringStiffness").unit(N_MM),
                Slot::Real(&mut corner.spring_stiffness),
            );
            f(
                spring(Springs, "Helper spring length", "HelperSpringLength", "HelperSpringLength").unit(MM),
                Slot::Real(&mut corner.helper_spring_length),
            );
            f(
                spring(Springs, "Helper spring rate", "HelperSpringStiffness", "HelperSpringStiffness").unit(N_MM),
                Slot::Real(&mut corner.helper_spring_stiffness),
            );
            f(spring(Dampers, "Bump (slow)", "DampingBump", "DampBump").unit(NS_M), Slot::Real(&mut corner.bump));
            f(spring(Dampers, "Rebound", "DampingRebound", "DampRebound").unit(NS_M), Slot::Real(&mut corner.rebound));
            f(
                spring(Dampers, "Bump (fast)", "DampingBumpHighSpeed", "DampBumpHiSpeed").unit(NS_M),
                Slot::Real(&mut corner.fast_bump),
            );
            f(
                spring(Dampers, "Fast bump from", "BumpHighSpeedBreak", "DampSpeedBrake").unit(M_S),
                Slot::Real(&mut corner.fast_bump_threshold),
            );
        }
        for (w, (corner, wheel)) in self.corners.iter_mut().zip(WHEELS).enumerate() {
            let spec = triple(
                Tyres,
                "Pressure",
                &format!("Tyre{wheel}"),
                "Pressure",
                &format!("TyreOptions{wheel}"),
                "Pressure",
            );
            f(spec.unit(BAR).wrapping().at(w), Slot::Real(&mut corner.tyre_pressure));
        }
    }

    /// The specs, in [`Setup::visit`] order.
    pub fn specs() -> Vec<Spec> {
        let mut specs = Vec::new();
        Setup::default().visit(&mut |spec, _| specs.push(spec));
        specs
    }

    /// The setup a file's root group gives: every value must be there (a default file, `setups/d_<surface>.lsp`).
    pub fn parse(root: &[Node]) -> Result<Setup, String> {
        let mut setup = Setup::default();
        let missing = setup.layer(root)?;
        match missing.first() {
            Some(key) => Err(format!("the setup has no {key} ({} missing)", missing.len())),
            None => Ok(setup),
        }
    }

    /// The values a file's root group has, over this setup (the loader's layering); the keys it lacks.
    pub fn layer(&mut self, root: &[Node]) -> Result<Vec<String>, String> {
        let mut missing = Vec::new();
        let mut error = None;
        self.visit(&mut |spec, slot| match value(root, &spec.group, &spec.key) {
            None => missing.push(format!("{} {}", spec.group, spec.key)),
            Some(text) => match slot {
                Slot::Real(v) if is_float(text) => *v = atof(text),
                Slot::Unsigned(v) if is_integer(text) => *v = text.parse::<i64>().unwrap_or(0) as u32,
                _ => error = Some(format!("{} {}: {text} is not a number", spec.group, spec.key)),
            },
        });
        error.map_or(Ok(missing), Err)
    }

    /// The setup file of these values: what the game's loader takes over the default file.
    pub fn to_text(&self) -> String {
        let mut groups: Vec<(String, String)> = Vec::new();
        let mut setup = *self;
        setup.visit(&mut |spec, slot| {
            let text = match slot {
                Slot::Real(v) => real_text(*v),
                Slot::Unsigned(v) => v.to_string(),
            };
            let line = format!("  {:<31} {text}\n", spec.key);
            match groups.iter_mut().find(|(g, _)| *g == spec.group) {
                Some((_, body)) => body.push_str(&line),
                None => groups.push((spec.group, line)),
            }
        });
        let mut out = String::from("((\"CarSetup\"\n");
        for (group, body) in groups {
            out.push_str(&format!(" {group:<18} (\":-D\"\n{body}  )\n"));
        }
        out.push_str(" ))\n");
        out
    }

    /// The setup of a setup file's text, over `base`.
    pub fn from_text(base: &Setup, text: &str) -> Result<Setup, String> {
        let file = lsp::parse(text).map_err(|e| e.to_string())?;
        let root = root(&file).ok_or("the file has no root group")?;
        let mut setup = *base;
        setup.layer(root)?;
        Ok(setup)
    }

    /// Each value's range from the options file's root group (`r_<surface>.lsp`), in [`Setup::visit`] order.
    pub fn ranges(options: &[Node]) -> Result<Vec<Range>, String> {
        Setup::specs().iter().map(|spec| range(options, spec)).collect()
    }
}

fn range(options: &[Node], spec: &Spec) -> Result<Range, String> {
    let get = |key: &str| {
        value(options, &spec.options, key)
            .filter(|t| is_float(t))
            .map(atof)
            .ok_or_else(|| format!("{} has no {key}", spec.options))
    };
    let (min, max, step) = match spec.bounds {
        Bounds::Triple(prefix) => {
            (get(&format!("{prefix}Min"))?, get(&format!("{prefix}Max"))?, get(&format!("{prefix}Step"))?)
        }
        Bounds::TopMount => (0.0, get("nTopMountPositions")?, 1.0),
        Bounds::DropGear => (get("DropGearOptionsMin")?, get("DropGearOptionsMax")?, 1.0),
    };
    Ok(Range { min, max, step, wrap: spec.wrap })
}

/// A float as the shortest text the loader reads back to the same float (no exponent, which `IsFloat` refuses).
fn real_text(v: f32) -> String {
    let text = format!("{v}");
    debug_assert!(is_float(&text) && atof(&text).to_bits() == v.to_bits());
    text
}

/// `CTokenCursor::IsFloat` (0x52eed0): an optional sign, then digits with at most one dot.
fn is_float(t: &str) -> bool {
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    digits.bytes().all(|c| c.is_ascii_digit() || c == b'.') && digits.bytes().filter(|&c| c == b'.').count() <= 1
}

/// `CTokenCursor::IsInteger` (0x52ef40).
fn is_integer(t: &str) -> bool {
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    digits.bytes().all(|c| c.is_ascii_digit())
}

/// `atof`, rounded to the member's float.
fn atof(t: &str) -> f32 {
    t.parse::<f64>().unwrap_or(0.0) as f32
}

/// The items of a file's one top-level group, `(("Name" ...))`.
pub fn root(file: &[Node]) -> Option<&[Node]> {
    match file {
        [Node::List(outer)] => match outer.as_slice() {
            [Node::List(items)] => Some(items),
            _ => None,
        },
        _ => None,
    }
}

/// The first value token of `key` in the group named `group` of a root group.
fn value<'a>(root: &'a [Node], group: &str, key: &str) -> Option<&'a str> {
    let items = root.windows(2).find_map(|pair| match pair {
        [Node::Atom(name), Node::List(items)] if name == group => Some(items),
        _ => None,
    })?;
    items.windows(2).find_map(|pair| match pair {
        [Node::Atom(name), Node::Atom(value)] if name == key => Some(value.as_str()),
        _ => None,
    })
}

/// A car's default setup and its ranges, from the retail `Root/physics.rbz`.
#[derive(Debug, Clone, PartialEq)]
pub struct CarSetup {
    pub default: Setup,
    pub ranges: Vec<Range>,
    /// The drop gears' pinion and gear teeth (`GearListNN`), by drop gear id.
    pub gears: Vec<(u32, f32, f32)>,
}

impl CarSetup {
    /// The defaults and ranges of car `car` (`[CarNN]`) on `surface` (`gravel`, `tarmac` or `snow`).
    pub fn load(data: &rbr_formats::data::Data, car: usize, surface: &str) -> Result<CarSetup, String> {
        let archive = data.resolve("Root/physics.rbz").ok_or("Root/physics.rbz: not found")?;
        CarSetup::load_in(&archive, car, surface)
    }

    /// The same from the `physics.rbz` at `archive` (under NGP its own, whose slots hold the installed NGP cars).
    pub fn load_in(archive: &std::path::Path, car: usize, surface: &str) -> Result<CarSetup, String> {
        let folder = CAR_FOLDERS.get(car).ok_or(format!("car {car}: not a retail car"))?;
        let mut zip = rbr_formats::rbz::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
        let mut file = |name: String| -> Result<String, String> {
            let mut text = String::new();
            std::io::Read::read_to_string(
                &mut rbr_formats::rbz::by_name(&mut zip, &name).map_err(|e| format!("{name}: {e}"))?,
                &mut text,
            )
            .map_err(|e| format!("{name}: {e}"))?;
            Ok(text)
        };
        CarSetup::parse(&file(format!("{folder}setups/d_{surface}.lsp"))?, &file(format!("{folder}r_{surface}.lsp"))?)
    }

    /// The defaults and ranges from a surface's default setup (`setups/d_<surface>.lsp`) and options
    /// (`r_<surface>.lsp`): a slot folder's in `physics.rbz`, or an NGP car pack's.
    pub fn parse(defaults: &str, options: &str) -> Result<CarSetup, String> {
        let defaults = lsp::parse(defaults).map_err(|e| format!("the default setup: {e}"))?;
        let options = lsp::parse(options).map_err(|e| format!("the options: {e}"))?;
        let defaults = root(&defaults).ok_or("the default setup has no root group")?;
        let options = root(&options).ok_or("the options have no root group")?;
        Ok(CarSetup { default: Setup::parse(defaults)?, ranges: Setup::ranges(options)?, gears: gear_list(options) })
    }
}

/// The rows of the season's setup choice before a stage (`CSeasonChooseSetupUI` 0x5460b0, `DrawRow` 0x5466b0): the
/// default setup of the next stage's surface (`CSDEFA`, the surface from `E14_4a1330` on the stage's map), the current
/// setup (`CSCURR`), or one of the car's three save slots (`CSSLOT`, `rbr_formats::saves::setup_slots`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeasonSetup {
    Default,
    #[default]
    Current,
    Slot(u32),
}

impl SeasonSetup {
    /// The rows in the menu's order for `slots` save slots.
    pub fn rows(slots: u32) -> Vec<SeasonSetup> {
        [SeasonSetup::Default, SeasonSetup::Current].into_iter().chain((0..slots).map(SeasonSetup::Slot)).collect()
    }

    /// The setup the row gives: the surface's `default`, the `current` one, or the slot's text over the default
    /// (`LoadFromMemory` parses a slot over the setup in place; ours: over the surface's default, so a slot saved on
    /// another surface gives the same values whatever was loaded before).
    pub fn setup(self, default: &Setup, current: &Setup, slot_text: Option<&str>) -> Result<Setup, String> {
        match self {
            SeasonSetup::Default => Ok(*default),
            SeasonSetup::Current => Ok(*current),
            SeasonSetup::Slot(n) => Setup::from_text(default, slot_text.ok_or(format!("slot {} is empty", n + 1))?),
        }
    }

    pub fn label(self) -> String {
        match self {
            SeasonSetup::Default => "Default".into(),
            SeasonSetup::Current => "Current".into(),
            SeasonSetup::Slot(n) => format!("Slot {}", n + 1),
        }
    }
}

fn gear_list(options: &[Node]) -> Vec<(u32, f32, f32)> {
    (0..)
        .map_while(|n| {
            let get = |key: &str| value(options, "DriveOptions", &format!("GearList{n:02}{key}")).map(atof);
            Some((n, get("Pinion")?, get("Gear")?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_clamps_or_wraps_as_the_sliders_do() {
        let clamp = Range { min: 100000.0, max: 300000.0, step: 1000.0, wrap: false };
        assert_eq!(clamp.up(220000.0), 221000.0);
        assert_eq!(clamp.up(299500.0), 300000.0);
        assert_eq!(clamp.down(100500.0), 100000.0);
        // Down from above the maximum takes the minimum (0x54ef12: the minimum unless it wraps).
        assert_eq!(clamp.down(400000.0), 100000.0);
        let wrap = Range { wrap: true, ..clamp };
        assert_eq!(wrap.up(300000.0), 100000.0);
        assert_eq!(wrap.down(100000.0), 300000.0);
        // The float adds of the original, not a grid: 0.31 less one 0.005 step is not 0.305.
        let length = Range { min: 0.15, max: 0.35, step: 0.005, wrap: false };
        assert_eq!(length.down(0.31), 0.31f32 - 0.005f32);
        assert_eq!(length.toward(0.31, 0.2), (0..22).fold(0.31f32, |v, _| length.down(v)));
    }

    #[test]
    fn the_text_reads_back() {
        let mut setup = Setup::default();
        let mut k = 0u32;
        setup.visit(&mut |_, slot| {
            k += 1;
            match slot {
                Slot::Real(v) => *v = k as f32 * 0.37 - 20.0,
                Slot::Unsigned(v) => *v = k,
            }
        });
        let text = setup.to_text();
        assert_eq!(Setup::parse(root(&lsp::parse(&text).unwrap()).unwrap()), Ok(setup));
        assert_eq!(Setup::from_text(&Setup::default(), &text), Ok(setup));
    }

    #[test]
    fn the_season_rows_give_the_default_current_or_a_slot() {
        let default = Setup { steering_lock: 1.4, drop_gear: 11, ..Setup::default() };
        let current = Setup { drop_gear: 12, ..default };
        let slot = "((\"CarSetup\" Drive (\":-D\" DropGearId 10 ) ))";
        assert_eq!(SeasonSetup::rows(3).len(), 5);
        assert_eq!(SeasonSetup::Default.setup(&default, &current, None), Ok(default));
        assert_eq!(SeasonSetup::Current.setup(&default, &current, None), Ok(current));
        assert_eq!(SeasonSetup::Slot(0).setup(&default, &current, Some(slot)).map(|s| s.drop_gear), Ok(10));
        assert!(SeasonSetup::Slot(1).setup(&default, &current, None).is_err());
    }

    #[test]
    fn a_file_layers_over_a_setup() {
        let base = Setup { steering_lock: 1.4, drop_gear: 11, ..Setup::default() };
        let text = "((\"CarSetup\" Drive (\":-D\" DropGearId 12 ) TyreRB (\":-D\" Pressure 210000 ) ))";
        let setup = Setup::from_text(&base, text).unwrap();
        assert_eq!((setup.steering_lock, setup.drop_gear, setup.corners[3].tyre_pressure), (1.4, 12, 210000.0));
        assert!(Setup::from_text(&base, "((\"CarSetup\" Drive (\":-D\" DropGearId 1.5 ) ))").is_err());
    }
}
