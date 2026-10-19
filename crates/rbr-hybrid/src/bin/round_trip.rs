//! Checks the `object!` tables on snapshots: `lower(lift(image)) == image` on every owned byte of the car's tabled
//! objects (`rbr_physics::store::car_objects`). The boot images are checked by `rbr-standalone --twin`.
//!
//! Usage: round_trip SNAPSHOT...

use rbr_physics::car_body::store::CarStore;
use std::process::ExitCode;

use rbr_hybrid::pure::CAR_PTR;
use rbr_physics::snapshot::Snapshot;
use rbr_physics::spring_damper::layout as spring;
use rbr_physics::store;
use rbr_physics::vcu::layout as vcu;

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: round_trip SNAPSHOT...");
        return ExitCode::from(2);
    }
    let mut ok = true;
    for path in &paths {
        let mem = match Snapshot::open(path) {
            Ok(snap) => snap.mem(),
            Err(e) => {
                println!("{path}: {e}");
                ok = false;
                continue;
            }
        };
        let car = mem.u32(CAR_PTR);
        let objects = store::car_objects(&mem, car);
        // The owners the tables name: each strut state's owner is its wheel, the VCU state's is the car.
        let wheels = mem.wheel_pointers(car);
        for o in &objects {
            let owner = match o.name.as_str() {
                "vcu state" => Some((vcu::StateObj::owner.load(&mem, o.at), car)),
                name if name.ends_with("strut state") => {
                    let k = usize::from(name.as_bytes()[5] - b'0');
                    Some((spring::StateObj::owner.load(&mem, o.at), wheels[k]))
                }
                _ => None,
            };
            if let Some((found, expected)) = owner.filter(|(found, expected)| found != expected) {
                println!("{path}: {} at {:#010x}: owner {found:#010x}, expected {expected:#010x}", o.name, o.at);
                ok = false;
            }
        }
        match store::round_trip(&mem, &objects) {
            Ok(bytes) => println!("{path}: {} objects, {bytes} owned bytes equal after lower(lift)", objects.len()),
            Err(e) => {
                println!("{path}: {e}");
                ok = false;
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
