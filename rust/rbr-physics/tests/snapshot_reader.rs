use rbr_physics::snapshot::{Region, Snapshot};
use std::path::{Path, PathBuf};
use std::process::Command;

const STEP_NAMES: [&str; 9] = [
    "vcu-cov.snap-300.bin",
    "vcu-cov.snap-1000.bin",
    "vcu-cov.snap-1600.bin",
    "vcu-cov.snap-2000.bin",
    "vcu-cov.snap-2400.bin",
    "vcu-cov.snap-2800.bin",
    "vcu-cov.snap-3200.bin",
    "vcu-cov.snap-10000.bin",
    "vcu-cov.snap-11000.bin",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn step_snapshot_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("RBR_STEP_SNAPSHOT_DIR") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([PathBuf::from("/tmp/rbr-snaps"), Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../snaps")]);
    candidates.into_iter().find(|dir| STEP_NAMES.iter().all(|name| dir.join(name).is_file()))
}

fn snapat_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("RBR_SNAPAT_DIR") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([
        workspace_root().join("vectors/dumps/h1-tanner-corolla/snapat"),
        workspace_root().join("../vectors/dumps/h1-tanner-corolla/snapat"),
    ]);
    candidates.into_iter().find(|dir| dir.is_dir())
}

fn parse_hex(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0, "odd-length hex data");
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn parse_u32(s: &str) -> u32 {
    s.parse().unwrap()
}

#[test]
fn matches_replay_py_for_step_and_load_time_snapshots() {
    let Some(step_dir) = step_snapshot_dir() else {
        eprintln!("skipped snapshot parity: the nine vcu-cov step snapshots are absent");
        return;
    };
    let Some(snapat_dir) = snapat_dir() else {
        eprintln!("skipped snapshot parity: the h1-tanner-corolla snapat directory is absent");
        return;
    };
    let load_dump = ["RBRRecompSnapAt-004ea720-1.bin", "RBRRecompSnapAt-0048d940-1.bin"]
        .into_iter()
        .map(|name| snapat_dir.join(name))
        .find(|path| path.is_file());
    let Some(load_dump) = load_dump else {
        eprintln!("skipped snapshot parity: no selected version-2 dump is present");
        return;
    };

    let paths: Vec<PathBuf> =
        STEP_NAMES.iter().map(|name| step_dir.join(name)).chain(std::iter::once(load_dump)).collect();
    let helper = workspace_root().join("harness/snapshot_reference.py");
    let output = Command::new("python3")
        .arg("-I")
        .arg("-B")
        .arg(helper)
        .args(&paths)
        .output()
        .expect("run the replay.py snapshot reference helper with python3 -I -B");
    assert!(output.status.success(), "snapshot reference helper failed:\n{}", String::from_utf8_lossy(&output.stderr));

    let text = String::from_utf8(output.stdout).expect("reference helper output is UTF-8");
    let mut lines = text.lines().peekable();
    for path in paths {
        let rust = Snapshot::open(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let name = path.file_name().unwrap().to_string_lossy();
        assert_eq!(lines.next(), Some(format!("snapshot\t{name}").as_str()));

        let header = rust.metadata();
        let row = lines.next().expect("reference header row");
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields[0], "header");
        assert_eq!(fields[1].parse::<u32>().unwrap(), header.version);
        assert_eq!(fields[2].parse::<u32>().unwrap(), header.step);
        assert_eq!(fields[3].parse::<u32>().unwrap(), header.substeps);
        assert_eq!(fields[4].parse::<u32>().unwrap(), header.thread);
        assert_eq!(fields[5].parse::<u32>().unwrap(), header.teb);
        assert_eq!(fields[6].parse::<u32>().unwrap(), header.stack_lo);
        assert_eq!(fields[7].parse::<usize>().unwrap(), rust.stack().len());
        assert_eq!(fields[8].parse::<u32>().unwrap(), u32::from(header.returns));

        let row = lines.next().expect("reference registers row");
        let mut fields = row.split('\t');
        assert_eq!(fields.next(), Some("registers"));
        let registers: Vec<u32> = fields.map(parse_u32).collect();
        assert_eq!(registers, header.registers.words());

        let row = lines.next().expect("reference fxsave row");
        let mut fields = row.split('\t');
        assert_eq!(fields.next(), Some("fxsave"));
        assert_eq!(parse_hex(fields.next().unwrap()), rust.fxsave());

        let mut reference_regions = Vec::new();
        while lines.peek().is_some_and(|line| line.starts_with("region\t")) {
            let mut fields = lines.next().unwrap().split('\t');
            assert_eq!(fields.next(), Some("region"));
            let words: Vec<u32> = fields.map(parse_u32).collect();
            assert_eq!(words.len(), 6);
            reference_regions.push(Region {
                base: words[0],
                size: words[1],
                file_offset: words[2],
                protect: words[3],
                kind: words[4],
                allocation_base: words[5],
            });
        }
        assert_eq!(reference_regions, rust.regions(), "region table: {}", path.display());

        let mem = rust.mem();
        let mut samples = Vec::new();
        while lines.peek().is_some_and(|line| line.starts_with("sample\t")) {
            let mut fields = lines.next().unwrap().split('\t');
            assert_eq!(fields.next(), Some("sample"));
            let label = fields.next().unwrap();
            let address = u32::from_str_radix(fields.next().unwrap().trim_start_matches("0x"), 16).unwrap();
            let expected = parse_hex(fields.next().unwrap());
            assert_eq!(mem.read(address, expected.len()), expected, "sample {label} in {}", path.display());
            samples.push(label.to_owned());
        }
        if matches!(header.version, 1 | 2) {
            assert!(samples.contains(&"car_pointer".to_owned()), "car pointer sample missing");
            assert!(samples.contains(&"car".to_owned()), "car sample missing");
            assert!(samples.contains(&"wheel_ptrs".to_owned()), "wheel pointers sample missing");
            for index in 0..4 {
                assert!(samples.contains(&format!("wheel{index}")), "wheel{index} sample missing");
            }
        }
        assert_eq!(lines.next(), Some("end"));

        if header.version == 1 && header.step == 300 {
            let car = mem.u32(0x8ef660);
            let before = mem.read(car, 4);
            let mut edited = mem.clone();
            let mut replacement = before.clone();
            replacement[0] ^= 0xff;
            edited.map(car, &replacement);
            assert_eq!(mem.read(car, 4), replacement, "cloned Mem shares written pages");
            assert_eq!(rust.mem().read(car, 4), before, "a fresh Mem sees the original mmap bytes");
        }
    }
    assert!(lines.next().is_none(), "unexpected output after the final snapshot");
}
