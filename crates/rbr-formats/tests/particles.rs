//! `RBR_DATA`: every retail `PS_*.ini` parses with every name reference resolved (but two), every `Tracks.ini` particle file
//! exists, and every sprite id names a texture in `Misc/particletextures.ini` that `Textures/particles.dat` holds.

use rbr_formats::{dat, ini, pss, rbz};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

fn install() -> Option<PathBuf> {
    std::env::var_os("RBR_DATA").map(PathBuf::from)
}

fn particle_files(maps: &std::path::Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(maps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
            name.starts_with("ps_") && name.ends_with(".ini")
        })
        .collect();
    files.sort();
    files
}

#[test]
fn every_retail_particle_file_parses_and_resolves() {
    let Some(install) = install() else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let files = particle_files(&install.join("Maps"));
    assert_eq!(files.len(), 12, "six rallies, full and _LOW");
    let mut archive = rbz::open(&install.join("Root/misc.rbz")).unwrap();
    let mut text = String::new();
    archive.by_name("Misc/particletextures.ini").unwrap().read_to_string(&mut text).unwrap();
    let textures = pss::parse_textures(&text).unwrap();
    assert_eq!(textures.len(), 61);
    let dat_bytes = std::fs::read(install.join("Textures/particles.dat")).unwrap();
    let sprites: BTreeSet<String> = dat::parse(&dat_bytes)
        .unwrap()
        .into_iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.path.rsplit('/').next().unwrap().to_ascii_lowercase())
        .collect();
    for name in &textures {
        assert!(sprites.contains(&pss::sprite_file(name).to_ascii_lowercase()), "{name} is not in particles.dat");
    }
    let mut dangling = BTreeSet::new();
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        let file = pss::parse(&String::from_utf8_lossy(&bytes)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let shown = path.display();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        for name in file.unresolved() {
            dangling.insert(format!("{stem}: {name}"));
        }
        assert!(!file.classes.is_empty() && !file.emitters.is_empty() && !file.handlers.is_empty(), "{shown}");
        for class in &file.classes {
            assert!((class.sprite as usize) < textures.len(), "{shown}: {} sprite {}", class.name, class.sprite);
            assert!(class.life >= 0.0, "{shown}: {}", class.name);
        }
        let named = pss::NAMED_EMITTERS.iter().filter(|name| file.emitter(name).is_some()).count();
        assert!(named >= 8, "{shown}: {named} of the named emitters");
        for name in ["g_debris_b_emitter", "g_debris_smoke_emitter", "g_exhaust_broken_emitter", "g_pop_emitter"] {
            assert!(file.emitter(name).is_some(), "{shown}: no {name}");
        }
        let exhaust = ["g_exhaust_emitter", "g_exhaust_arctic_emitter"];
        assert!(exhaust.iter().any(|name| file.emitter(name).is_some()), "{shown}: no exhaust");
        for emitter in &file.emitters {
            assert!(file.emitter_particles(emitter).is_some(), "{shown}: {} has no particle class", emitter.name);
        }
    }
    // The one dangling reference in the retail data: PS_British's four bush damping handlers name emitters the file
    // does not define (PS_British_LOW has no damping handlers).
    let expected = ["PS_British: emitter: ho_DAMP_grass1_emitter", "PS_British: emitter: ho_DAMP_grass2_emitter"];
    assert_eq!(dangling, expected.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>());
}

#[test]
fn every_track_names_an_existing_particle_file() {
    let Some(install) = install() else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let maps = install.join("Maps");
    let present: BTreeSet<String> =
        particle_files(&maps).iter().map(|p| p.file_stem().unwrap().to_string_lossy().to_ascii_lowercase()).collect();
    let text = std::fs::read_to_string(maps.join("Tracks.ini")).unwrap();
    let mut named = 0;
    for section in ini::parse(&text).unwrap() {
        let Some(value) = section.get("Particles") else { continue };
        let stem = value.trim_matches('"').rsplit('\\').next().unwrap().to_ascii_lowercase();
        assert!(present.contains(&stem), "[{}] {stem}", section.name);
        assert!(present.contains(&format!("{stem}_low")), "[{}] {stem}_low", section.name);
        named += 1;
    }
    assert_eq!(named, 65);
}
