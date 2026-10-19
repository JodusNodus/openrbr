//! Reads every ghost in the retail `Root/ghosts.rbz` (`rbr_formats::ghost`) and writes it back.
//!
//! Runs only when `RBR_DATA` points at the extracted install tree (`original/retail-1.0/install`).

use rbr_formats::ghost::{Ghost, Sample, QUAT_SCALE};
use rbr_formats::rbz;
use std::io::Read;
use std::path::Path;

#[test]
fn every_retail_ghost_parses_and_round_trips() {
    let Ok(data) = std::env::var("RBR_DATA") else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let mut archive = rbz::open(&Path::new(&data).join("Root/ghosts.rbz")).expect("ghosts.rbz");
    let (mut rb, mut rse) = (0, 0);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        if !name.ends_with(".gho") {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let ghost = Ghost::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let written = ghost.write();
        let first = written.iter().zip(&bytes).position(|(a, b)| a != b);
        assert!(first.is_none() && written.len() == bytes.len(), "{name}: written back differs at {first:?}");
        let stem = name.rsplit('/').next().unwrap().trim_end_matches(".gho");
        if let Some(map) = stem.strip_prefix("rb_") {
            assert_eq!(ghost.map.to_string(), map, "{name}: the map is the file's number");
            assert_eq!(ghost.car, 5, "{name}: Richard Burns drives the Impreza 2003 (Car05)");
            rb += 1;
        } else {
            rse += 1;
        }
        assert_eq!(ghost.reserved, [0xff; 12], "{name}");
        let samples = &ghost.stream.samples;
        assert!(matches!(samples.first(), Some(Sample::Key(_))), "{name}: starts with a key");
        assert!(samples.windows(2).all(|w| w[0].time() <= w[1].time()), "{name}: times never go back");
        for s in samples {
            let q = match s {
                Sample::Key(k) => k.rotation,
                Sample::Delta(d) => d.rotation,
            };
            let norm = q.iter().map(|&v| (v as f32 / QUAT_SCALE).powi(2)).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 0.03, "{name}: quaternion norm {norm}");
        }
        let keys = samples.iter().filter(|s| matches!(s, Sample::Key(_))).count();
        eprintln!(
            "{name}: map {}, car {}, {} samples ({keys} keys), {:.2} s, tail {} bytes, {} after",
            ghost.map,
            ghost.car,
            samples.len(),
            samples.last().unwrap().seconds(),
            ghost.stream.tail.len(),
            ghost.after.len()
        );
    }
    // 20 ghosts: the archive's other 12 entries are `rse*.arp` and `demo1.rpl`.
    assert_eq!((rb, rse), (7, 13));
}
