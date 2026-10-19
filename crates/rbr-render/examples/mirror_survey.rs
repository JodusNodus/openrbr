//! Lists the mirrors `rbr_render::mirrors::find` finds in each retail car's interior set and, with `RBR_RSF`, each
//! RSF sample car's. Usage: `mirror_survey [CAR...]` (RSF cars by id or pack).

use rbr_formats::data::Data;
use rbr_render::car;
use rbr_render::mirrors;
use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let rsf = std::env::var_os("RBR_RSF").map(PathBuf::from);
    let mut cars: Vec<(String, Data, String)> = Vec::new();
    let retail = ["XSARA", "ACCENT", "ZR", "LANCER_EVO_VII", "PEUGEOT_206", "IMPREZA03", "COROLLA", "IMPREZA00"];
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in retail {
        if wanted.is_empty() || wanted.iter().any(|w| w.eq_ignore_ascii_case(name)) {
            cars.push((name.to_owned(), data.clone(), name.to_owned()));
        }
    }
    if let Some(root) = &rsf {
        let install = rbr_formats::rsf::Install::open(root)?;
        for car in &install.cars {
            let key = car.id.to_string();
            if !wanted.is_empty() && !wanted.contains(&key) {
                continue;
            }
            match rbr_formats::rsf::select(data.clone(), &key, rbr_formats::rsf::SLOT, Some(root)) {
                Ok(selected) => {
                    let folder = selected.folder().to_owned();
                    cars.push((format!("rsf {} {}", car.id, car.name), selected.data, folder))
                }
                Err(e) if e.contains("no pack") => {}
                Err(e) => println!("rsf {} {}: {e}", car.id, car.name),
            }
        }
    }
    for (label, data, folder) in cars {
        let interior = match car::load_interior(&data, &folder, car::placement_at(None)) {
            Ok(interior) => interior,
            Err(e) => {
                println!("{label}: {e:?}");
                continue;
            }
        };
        let exterior = match car::load(&data, &folder, car::placement_at(None)) {
            Ok(exterior) => exterior,
            Err(e) => {
                println!("{label}: {e:?}");
                continue;
            }
        };
        let (lo, hi) = exterior.bounds().ok_or("no geometry")?;
        let mounts = rbr_render::views::Mounts::load(&data, &folder, lo, hi, &exterior.meshes, &interior.meshes)?;
        let found = mirrors::find(&interior.meshes, &interior.damage, mounts.cockpit.eye);
        let list: Vec<String> = found
            .mirrors
            .iter()
            .map(|m| {
                format!(
                    "{:?} meshes {:?} ({} tris) at ({:.2} {:.2} {:.2}) n ({:.2} {:.2} {:.2}) {:.0}x{:.0} cm",
                    m.kind,
                    m.faces.iter().map(|f| f.0).collect::<std::collections::BTreeSet<_>>(),
                    m.faces.len(),
                    m.centre.x,
                    m.centre.y,
                    m.centre.z,
                    m.normal.x,
                    m.normal.y,
                    m.normal.z,
                    m.half.x * 200.0,
                    m.half.y * 200.0
                )
            })
            .collect();
        let eye = mounts.cockpit.eye;
        println!("{label} [{folder}]: {} mirrors, eye ({:.2} {:.2} {:.2})", found.mirrors.len(), eye.x, eye.y, eye.z);
        for line in list {
            println!("    {line}");
        }
        for u in &found.unfitted {
            println!(
                "    unfitted: mesh {} ({} tris) {:.1} mm off its plane",
                u.mesh,
                u.triangles,
                u.off_plane * 1000.0
            );
        }
    }
    Ok(())
}
