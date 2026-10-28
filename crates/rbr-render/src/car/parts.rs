//! Which `.sgc` parts a car set draws: the wheels by tyre class, the detachable and hinged parts, and the part
//! sections of the exterior, interior and ghost sets ([`Skip`] for the rest).

use super::{
    Skip, GHOST_PREFIX, INTERIOR_PREFIX, LOADER_DROPS, OVERLAY_PREFIX, PLANE_PREFIX, PREMOVE_PART, SURFACE,
    SURFACE_PARTS, UNBUILT_PREFIXES,
};
use crate::lights;
use rbr_formats::car_parts;
use rbr_formats::ini::Section;
use rbr_recording::detach;

/// The tyre class and wheel (0 to 3, `crate::playback`'s order) of an `e_wheel_<class>_<side>` node or of its
/// `e_wheel_<class>_tyre_<side>` child. The model's `+X` side (`l`) is the body's `+X`, where wheels 0 and 2 are.
pub(crate) fn wheel_of(name: &str) -> Option<(&str, usize)> {
    let (class, side) = name.strip_prefix("e_wheel_")?.split_once('_')?;
    let side = side.strip_prefix("tyre_").unwrap_or(side);
    let k = match side {
        "lf" => 0,
        "rf" => 1,
        "lb" => 2,
        "rb" => 3,
        _ => return None,
    };
    Some((class, k))
}

/// The detachable part a mesh is ([`detach::PARTS`]): an exterior part by its name, and an interior panel by its
/// exterior twin's (`i_door_l` goes with `e_door_l`; ours: HH flags the exterior node, and whether the interior one
/// goes with it is not traced).
pub(super) fn detach_part(name: &str) -> Option<u32> {
    let exterior = name.strip_prefix(INTERIOR_PREFIX).map(|rest| format!("e_{rest}"));
    detach::part_index(exterior.as_deref().unwrap_or(name))
}

/// The side doors and their interior panels, with the side sign of their hinge (see
/// [`coords::hinged_panel_to_car`]): the left door's hinge axis is up, the right's is down.
pub(super) fn hinge_side(name: &str) -> Option<(&'static str, f32)> {
    match name {
        "e_door_l" => Some(("l", 1.0)),
        "i_door_l" => Some(("l", 1.0)),
        "e_door_r" => Some(("r", -1.0)),
        "i_door_r" => Some(("r", -1.0)),
        _ => None,
    }
}

pub(super) fn part_for<'a>(name: &str, parts: &'a [Section]) -> Result<&'a Section, Skip> {
    part_in_set(name, parts, false)
}

/// [`part_for`] for the interior set: the `i_` parts it skips, and none of the others.
pub(super) fn interior_part_for<'a>(name: &str, parts: &'a [Section]) -> Result<&'a Section, Skip> {
    part_in_set(name, parts, true)
}

/// [`part_for`] for the ghost set: only the `ghost_` parts.
pub(super) fn ghost_part_for<'a>(name: &str, parts: &'a [Section]) -> Result<&'a Section, Skip> {
    if !name.starts_with(GHOST_PREFIX) {
        return Err(Skip::NotGhost);
    }
    car_parts::section(parts, name).ok_or(Skip::NoSection)
}

fn part_in_set<'a>(name: &str, parts: &'a [Section], interior: bool) -> Result<&'a Section, Skip> {
    if name.starts_with(GHOST_PREFIX) {
        return Err(Skip::Ghost);
    }
    if name.starts_with(OVERLAY_PREFIX) {
        return Err(Skip::Overlay);
    }
    if name.starts_with(PLANE_PREFIX) {
        return Err(Skip::Plane);
    }
    if name.starts_with("i_") && LOADER_DROPS.iter().any(|p| name.contains(p))
        || name == PREMOVE_PART
        || UNBUILT_PREFIXES.iter().any(|p| name.starts_with(p))
    {
        return Err(Skip::LoaderDrops);
    }
    if name.starts_with(INTERIOR_PREFIX) != interior {
        return Err(if interior { Skip::Exterior } else { Skip::Interior });
    }
    let part = car_parts::section(parts, name).ok_or(Skip::NoSection)?;
    // A lamp's part is drawn in the state its lamp is in (`lights::Switch`), so only the other switch parts are hidden.
    if lights::Switch::of_part(part).is_none() && !shown_at_start(part) {
        return Err(Skip::Switched);
    }
    if SURFACE_PARTS.iter().any(|p| name.contains(p)) && !name.contains(SURFACE) {
        return Err(Skip::SurfaceWheel);
    }
    if car_parts::face_blocks(part).is_empty() {
        return Err(Skip::NoShaderSlots);
    }
    Ok(part)
}

/// Whether a part is shown at the stage start. The loader makes a part with `Switch = true` and a non-empty
/// `SwitchBox` and `Predicate` a `CGeomObjectSwitch` (`FUN_1001fc70`, `FUN_1001fde0`), hidden until its box is set to
/// its predicate (`FUN_10043090`). `CHH_World`'s vf14 (0x100690a0) sets every box whose name contains `light`, `brake`
/// or `reverse` to `off` (its table also lists `lightglass` and `brakedisc`, which those already match); other boxes
/// stay unset. So the unlit lights and brake discs (`Predicate = off`) are drawn and the lit ones are not.
fn shown_at_start(part: &Section) -> bool {
    let value = |key| part.get(key).filter(|v| !v.is_empty());
    match (car_parts::is_switched(part), value("SwitchBox"), value("Predicate")) {
        (true, Some(switch_box), Some(predicate)) => {
            ["light", "brake", "reverse"].iter().any(|n| switch_box.contains(n)) && predicate == "off"
        }
        _ => true,
    }
}
