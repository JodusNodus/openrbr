//! The load tests' built objects as a patch file for `harness/compose_load.py` (its docstring has the format and the
//! classes): with `RBR_LOAD_OUT` set to a directory, a test writes its objects to `<dir>/<name>.patch`.

#![allow(dead_code)]

use std::io::Write;

/// One built object: its kind, address, our bytes (the build on 0x00), the class of each byte and the game's bytes it
/// was compared with (ours where the game's are not at hand).
pub struct Patched {
    pub kind: String,
    pub at: u32,
    pub ours: Vec<u8>,
    pub class: Vec<u8>,
    pub game: Vec<u8>,
}

/// Writes `objects` to `<RBR_LOAD_OUT>/<name>.patch` when `RBR_LOAD_OUT` is set.
pub fn write(name: &str, objects: &[Patched]) {
    let Some(dir) = std::env::var_os("RBR_LOAD_OUT") else {
        return;
    };
    let path = std::path::Path::new(&dir).join(format!("{name}.patch"));
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    for o in objects {
        assert!(o.ours.len() == o.class.len() && o.ours.len() == o.game.len());
        out.write_all(&o.at.to_le_bytes()).unwrap();
        out.write_all(&(o.ours.len() as u32).to_le_bytes()).unwrap();
        out.write_all(&[o.kind.len() as u8]).unwrap();
        out.write_all(o.kind.as_bytes()).unwrap();
        out.write_all(&o.ours).unwrap();
        out.write_all(&o.class).unwrap();
        out.write_all(&o.game).unwrap();
    }
    eprintln!("wrote {} objects to {}", objects.len(), path.display());
}
