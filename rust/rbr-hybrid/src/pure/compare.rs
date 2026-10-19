//! The car and wheel words of a recording frame and their compare with a run (`rust_step.py`'s masks).

use std::collections::{BTreeMap, BTreeSet};

use rbr_gameplay::tick::SETTLE_SUBSTEPS;
use rbr_physics::mem::Mem;
use rbr_physics::snapshot::Snapshot;
use rbr_recording::{Frame, CONTROL_BYTES, WHEELS, WHEEL_BYTES};

use super::heap::{ALLOCATOR_HEAP_BASE, ALLOCATOR_HEAP_END};
use super::image::{CAR_BYTES, CAR_PTR};

/// The car and wheel words of a recording frame.
pub const FRAME_WORDS: usize = (CAR_BYTES + WHEELS * WHEEL_BYTES) / 4;

/// The car's stale-stack words, rust_step.py's `CAR_STATIC` (byte offsets in the car, the same list): the lane-3 words
/// (offset 0xc mod 16) the original's own stale-stack runs or its recording differ at, and
/// 0x15dc, which `CCar::Vf64x` (0x4e9020) blends from 0x38c's lane 3.
const STALE_CAR: [usize; 23] = [
    0x1cc, 0x8ec, 0x90c, 0x147c, 0x149c, 0x14bc, 0x14cc, 0x14ec, 0x150c, 0x161c, 0x22c, 0x32c, 0x38c, 0x65c, 0x19c,
    0x20c, 0x2ec, 0x34c, 0x8c, 0x9c, 0x14c, 0x33c, 0x15dc,
];
/// A wheel's stale-stack words, rust_step.py's `STACK_GARBAGE` (the lane-3 words of the wheel's vectors).
const STALE_WHEEL: [usize; 4] = [0x99c, 0x9bc, 0x9dc, 0x73c];
/// The car's frame-loop words, in rust_step.py's `RECORDING_GARBAGE`. `CCar::Vf28x` (0x4ea020) runs once per rendered
/// frame, outside the physics step; its drive call (0x508480, write at 0x5084ab) writes car+0x660 and car+0x670, and
/// its engine call (0x518cf0, write at 0x518d26) writes car+0x814. The direct records show +0x660 and +0x814 changing
/// together.
const FRAME_LOOP_CAR: [usize; 3] = [0x660, 0x670, 0x814];
/// The w lane of the wheel's contact point (+0x6e0). The ground-probe blend (0x53dc60) takes it from the w of the strut
/// ray's start point (+0xa3c), which the original never initialises, so it holds that process's heap residue: separate
/// original runs of one drive differ only here. Recordings do not hold +0xa3c.
const HEAP_WHEEL: usize = 0x6ec;

/// Under NGP, the w lane of a wheel body's position (`CSolid+0x140`) and of its window start (`+0x330`), the same
/// lane-3 words as the car's `0x14c` and `0x33c` in [`STALE_CAR`]. Under NGP the wheels are bodies that `E43Vf6C`
/// integrates; its lanewise SSE step (`rigid_body::integrate`) carries the original's stack residue into w, and the
/// wheel's `refresh` (`wheel::layout::refresh`) stores the whole vector back (the two writers a write watch finds). So
/// the original's w is residue (0.22, 44, -1e12 in ngp-snow-206-15) where the pure one is about 0.11, at every step from
/// the boot, while the x, y and z lanes and every other word stay equal over 130038 steps; the port reads that w only
/// into itself (`rigid_body::pose`'s lanewise step, `v.w * a + position.w`). Counted apart
/// ([`Compare::mask_ngp_padding`]), not compared.
pub const NGP_PADDING_WHEEL: [usize; 2] = [0x14c, 0x33c];

/// Whether frame word `i` is one of the wheels' [`NGP_PADDING_WHEEL`] words.
pub fn is_ngp_padding(i: usize) -> bool {
    let at = 4 * i;
    at >= CAR_BYTES && NGP_PADDING_WHEEL.contains(&((at - CAR_BYTES) % WHEEL_BYTES))
}

/// The stale-stack class of the car and the wheels ([`STALE_CAR`], [`STALE_WHEEL`]) and the wheels' heap word
/// ([`HEAP_WHEEL`]): the original's own residue, not compared; and the car's frame-loop words ([`FRAME_LOOP_CAR`]). [`Compare::diff`] counts the words that differ there apart ([`Diff::stale`]).
pub fn stale_mask() -> Vec<bool> {
    let mut mask = vec![false; FRAME_WORDS];
    for off in STALE_CAR.into_iter().chain(FRAME_LOOP_CAR) {
        mask[off / 4] = true;
    }
    for k in 0..WHEELS {
        for off in STALE_WHEEL.into_iter().chain([HEAP_WHEEL]) {
            mask[(CAR_BYTES + k * WHEEL_BYTES + off) / 4] = true;
        }
    }
    mask
}

/// `diverge.py`'s `static_mask` over frame words (control excluded): frame artifacts and every word that is ever NaN,
/// infinite or above 1e20 in the recording. Plus wheel+0x9ac (`rust_step.py`'s `RECORDING_GARBAGE`). The stale-stack
/// words are [`stale_mask`]'s.
pub fn static_mask(frames: &[Frame]) -> Vec<bool> {
    let mut mask = vec![false; FRAME_WORDS];
    let cw = CONTROL_BYTES / 4;
    for w in [0x844, 0x678, 0x694] {
        mask[w / 4 - cw] = true;
    }
    for k in 0..WHEELS {
        mask[(CAR_BYTES + k * WHEEL_BYTES + 0x9ac) / 4] = true;
    }
    for f in frames {
        for (i, w) in recorded_words(f).iter().enumerate() {
            let x = f32::from_bits(*w);
            if !x.is_finite() || x.abs() > 1e20 {
                mask[i] = true;
            }
        }
    }
    mask
}

/// A frame's car and wheel words, -0.0 as 0.0.
pub fn recorded_words(f: &Frame) -> Vec<u32> {
    let mut bytes = Vec::with_capacity(f.car.len() + WHEELS * WHEEL_BYTES);
    bytes.extend_from_slice(f.car);
    for k in 0..WHEELS {
        bytes.extend_from_slice(f.wheel(k).unwrap_or(&[0; WHEEL_BYTES]));
    }
    to_words(&bytes)
}

fn to_words(bytes: &[u8]) -> Vec<u32> {
    let (chunks, _) = bytes.as_chunks::<4>();
    chunks
        .iter()
        .map(|c| match u32::from_le_bytes(*c) {
            0x8000_0000 => 0,
            w => w,
        })
        .collect()
}

/// The car and wheel bytes in `mem`, as a recording frame holds them (a null wheel pointer is zeros).
pub fn frame_bytes(mem: &Mem) -> Vec<u8> {
    let car = mem.u32(CAR_PTR);
    let mut bytes = mem.read(car, CAR_BYTES);
    for k in 0..WHEELS as u32 {
        let w = mem.u32(car + 0x470 + 4 * k);
        bytes.extend(if w == 0 { vec![0; WHEEL_BYTES] } else { mem.read(w, WHEEL_BYTES) });
    }
    bytes
}

/// The car and wheel words in `mem`, as a recording frame holds them.
pub fn frame_words(mem: &Mem) -> Vec<u32> {
    to_words(&frame_bytes(mem))
}

/// Frame word `i` as car+off or wheel k+off.
pub fn word_name(i: usize) -> String {
    let at = 4 * i;
    if at < CAR_BYTES {
        format!("car+{at:#x}")
    } else {
        let (k, off) = ((at - CAR_BYTES) / WHEEL_BYTES, (at - CAR_BYTES) % WHEEL_BYTES);
        format!("wheel{k}+{off:#x}")
    }
}

/// `rust_step.py`'s compare of run A against the recording: words that differ between the snapshot and the recording
/// before the first step are noise, and so are the [`static_mask`]'s; a word that is a 4-aligned mapped address here
/// and a 4-aligned user address in the recording is a pointer, counted, not compared.
pub struct Compare {
    mask: Vec<bool>,
    /// [`stale_mask`]: counted apart, not compared.
    stale: Vec<bool>,
    /// Run-noise candidates and their pure/recording values when comparison began.
    noise: BTreeMap<usize, (u32, u32)>,
    /// Noise candidates leave this set permanently after either side changes its start value.
    released_noise: BTreeSet<usize>,
    /// Candidates released by a recorded change while the pure value still held its start value.
    recorded_noise_released: BTreeSet<usize>,
    /// Boot-time mapped pointers whose addresses differ from the recording's address space. These are relocation
    /// differences, not run noise; keep them separately so a later write is compared normally.
    boot_pointers: BTreeMap<usize, u32>,
    regions: Vec<(u32, u32)>,
}

/// One step's compare: the differing frame words, the snapshot pointer words, the stand-in placement words, and the
/// stale-stack words that differ (not failures).
pub struct Diff {
    pub words: Vec<usize>,
    pub pointers: usize,
    pub allocator_pointers: usize,
    pub stale: Vec<usize>,
}

/// Whether the snapshot's address and recording's address are a relocated pointer pair.
fn is_relocated_pointer(have: u32, want: u32, regions: &[(u32, u32)]) -> bool {
    let mapped = regions.iter().any(|&(base, size)| base <= have && have - base < size);
    let allocator = (ALLOCATOR_HEAP_BASE..ALLOCATOR_HEAP_END).contains(&have);
    have.is_multiple_of(4) && (mapped || allocator) && want.is_multiple_of(4) && (0x10000..0x8000_0000).contains(&want)
}

/// Split stable step-0 differences into run noise and relocated pointers.
fn split_boot_candidates(
    start: &[u32],
    settle: &[Vec<u32>],
    regions: &[(u32, u32)],
) -> (BTreeMap<usize, (u32, u32)>, BTreeMap<usize, u32>) {
    let mut noise = BTreeMap::new();
    let mut boot_pointers = BTreeMap::new();
    if let Some(first) = settle.first() {
        for i in 0..start.len() {
            if start[i] == first[i] || !settle.iter().all(|frame| frame[i] == first[i]) {
                continue;
            }
            if is_relocated_pointer(start[i], first[i], regions) {
                boot_pointers.insert(i, start[i]);
            } else {
                noise.insert(i, (start[i], first[i]));
            }
        }
    }
    (noise, boot_pointers)
}

/// Permanently release a noise candidate once either side changes its start value.
fn release_changed_noise(
    noise: &BTreeMap<usize, (u32, u32)>,
    released: &mut BTreeSet<usize>,
    recorded_released: &mut BTreeSet<usize>,
    have: &[u32],
    want: &[u32],
) {
    let mut changed = Vec::new();
    let mut recorded_changed = Vec::new();
    for (&i, &(start_have, start_want)) in noise {
        if released.contains(&i) {
            continue;
        }
        let pure_changed = have[i] != start_have;
        let recording_changed = want[i] != start_want;
        if recording_changed && !pure_changed {
            recorded_changed.push(i);
        }
        if pure_changed || recording_changed {
            changed.push(i);
        }
    }
    released.extend(changed);
    recorded_released.extend(recorded_changed);
}

impl Compare {
    /// `before` is the recorded frame of the step before the snapshot's.
    pub fn new(snap: &Snapshot, mask: Vec<bool>, before: &Frame) -> Self {
        let want = recorded_words(before);
        let have = frame_words(&snap.mem());
        let noise =
            want.iter().zip(&have).enumerate().filter(|(_, (w, h))| w != h).map(|(i, (&w, &h))| (i, (h, w))).collect();
        let regions: Vec<(u32, u32)> = snap.regions().iter().map(|r| (r.base, r.size)).collect();
        Compare {
            mask,
            stale: stale_mask(),
            noise,
            released_noise: BTreeSet::new(),
            recorded_noise_released: BTreeSet::new(),
            boot_pointers: BTreeMap::new(),
            regions,
        }
    }

    /// The compare for a snapshot at step 0, which no recorded frame ends at. Its run noise is the words the recorded
    /// load settle never changes (equal in every settle frame, the frames of `StepForward(40)` calls) where the
    /// snapshot holds another value. Such a word is noise only while both the pure and recorded values keep their
    /// step-0 values; once either changes, it is compared with the recording.
    pub fn at_start(snap: &Snapshot, mask: Vec<bool>, frames: &[Frame]) -> Self {
        let regions: Vec<(u32, u32)> = snap.regions().iter().map(|r| (r.base, r.size)).collect();
        Compare::at_start_of(&snap.mem(), regions, mask, frames)
    }

    /// [`Compare::at_start`] for the image `mem` whose mapped ranges are `regions` (a booted image's).
    pub fn at_start_of(mem: &Mem, regions: Vec<(u32, u32)>, mask: Vec<bool>, frames: &[Frame]) -> Self {
        let settle: Vec<Vec<u32>> =
            frames.iter().take_while(|f| f.substeps == SETTLE_SUBSTEPS[0]).map(recorded_words).collect();
        let start = frame_words(mem);
        let (noise, boot_pointers) = split_boot_candidates(&start, &settle, &regions);
        Compare {
            mask,
            stale: stale_mask(),
            noise,
            released_noise: BTreeSet::new(),
            recorded_noise_released: BTreeSet::new(),
            boot_pointers,
            regions,
        }
    }

    /// The number of run-noise words.
    pub fn noise(&self) -> usize {
        self.noise.len()
    }

    /// The boot-noise candidates that still hold their step-0 snapshot value.
    /// A changed word is compared with the recording from then on.
    pub fn kept_words(&self, mem: &Mem) -> Vec<usize> {
        let have = frame_words(mem);
        self.noise
            .iter()
            .filter_map(|(&i, &(value, _))| (!self.released_noise.contains(&i) && have[i] == value).then_some(i))
            .collect()
    }

    /// Counts the wheels' [`NGP_PADDING_WHEEL`] words apart, with the stale-stack words (an NGP run's compare).
    pub fn mask_ngp_padding(&mut self) {
        for i in (0..FRAME_WORDS).filter(|&i| is_ngp_padding(i)) {
            self.stale[i] = true;
        }
    }

    /// Noise candidates that have become subject to comparison.
    pub fn released_noise_words(&self) -> Vec<usize> {
        self.released_noise.iter().copied().filter(|&i| !self.mask[i]).collect()
    }

    /// Unmasked noise candidates released because the recording changed while pure kept its start value.
    pub fn recorded_noise_released_words(&self) -> Vec<usize> {
        self.recorded_noise_released.iter().copied().filter(|&i| !self.mask[i]).collect()
    }

    /// Boot-time relocated pointers that still hold their snapshot address.
    pub fn kept_boot_pointers(&self, mem: &Mem) -> Vec<usize> {
        let have = frame_words(mem);
        self.boot_pointers.iter().filter_map(|(&i, &value)| (have[i] == value).then_some(i)).collect()
    }

    pub fn diff(&mut self, mem: &Mem, frame: &Frame) -> Diff {
        let (want, have) = (recorded_words(frame), frame_words(mem));
        release_changed_noise(&self.noise, &mut self.released_noise, &mut self.recorded_noise_released, &have, &want);
        let mut out = Diff { words: vec![], pointers: 0, allocator_pointers: 0, stale: vec![] };
        for i in 0..FRAME_WORDS {
            let (h, w) = (have[i], want[i]);
            if h == w || self.mask[i] || (self.noise.contains_key(&i) && !self.released_noise.contains(&i)) {
                continue;
            }
            if self.stale[i] {
                out.stale.push(i);
                continue;
            }
            let allocator = (ALLOCATOR_HEAP_BASE..ALLOCATOR_HEAP_END).contains(&h);
            let is_relocated = is_relocated_pointer(h, w, &self.regions);
            if self.boot_pointers.get(&i) == Some(&h) && is_relocated {
                continue;
            }
            if is_relocated {
                if allocator {
                    out.allocator_pointers += 1;
                } else {
                    out.pointers += 1;
                }
            } else {
                out.words.push(i);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_counts_allocator_placement_words_separately() {
        let mut mem = Mem::new();
        let car = 0x20000;
        let recorded_pointer: u32 = 0x30000;
        mem.set_u32(CAR_PTR, car);
        mem.map(car, &[0; CAR_BYTES]);
        mem.set_u32(car + 0x100, ALLOCATOR_HEAP_BASE);

        let mut bytes = vec![0; rbr_recording::HEADER_BYTES + CONTROL_BYTES + CAR_BYTES + WHEELS * WHEEL_BYTES];
        bytes[0..4].copy_from_slice(&rbr_recording::MAGIC.to_le_bytes());
        bytes[16..20].copy_from_slice(&(CAR_BYTES as u32).to_le_bytes());
        let recorded_at = rbr_recording::HEADER_BYTES + CONTROL_BYTES + 0x100;
        bytes[recorded_at..recorded_at + 4].copy_from_slice(&recorded_pointer.to_le_bytes());
        let frame = rbr_recording::Frames::new(&bytes).next().unwrap().unwrap();
        let mut compare = Compare {
            mask: vec![false; FRAME_WORDS],
            stale: vec![false; FRAME_WORDS],
            noise: BTreeMap::new(),
            released_noise: BTreeSet::new(),
            recorded_noise_released: BTreeSet::new(),
            boot_pointers: BTreeMap::new(),
            regions: vec![(car, CAR_BYTES as u32)],
        };

        let diff = compare.diff(&mem, &frame);
        assert!(diff.words.is_empty());
        assert_eq!(diff.pointers, 0);
        assert_eq!(diff.allocator_pointers, 1);
    }

    #[test]
    fn step_zero_splits_relocated_pointer_candidates_from_run_noise() {
        let start = [0x20020, 0, 0x30004, 0x40000];
        let settle = vec![vec![0x30020, 0x3f80_0000, 0x50000, 0x40000]; 4];
        let regions = [(0x20000, 0x1000)];
        let (noise, pointers) = split_boot_candidates(&start, &settle, &regions);

        assert_eq!(pointers, [(0, 0x20020)].into_iter().collect());
        assert_eq!(noise, [(1, (0, 0x3F80_0000)), (2, (0x30004, 0x50000))].into_iter().collect());
    }

    #[test]
    fn recorded_change_releases_run_noise_permanently() {
        let noise = [(4, (10, 20)), (8, (30, 40))].into_iter().collect();
        let mut released = BTreeSet::new();
        let mut recorded_released = BTreeSet::new();
        let have = [0, 0, 0, 0, 10, 0, 0, 0, 30];
        let want = [0, 0, 0, 0, 20, 0, 0, 0, 40];

        release_changed_noise(&noise, &mut released, &mut recorded_released, &have, &want);
        assert!(released.is_empty());
        assert!(recorded_released.is_empty());

        let changed_want = [0, 0, 0, 0, 21, 0, 0, 0, 40];
        release_changed_noise(&noise, &mut released, &mut recorded_released, &have, &changed_want);
        assert_eq!(released, [4].into_iter().collect());
        assert_eq!(recorded_released, [4].into_iter().collect());

        release_changed_noise(&noise, &mut released, &mut recorded_released, &have, &want);
        assert_eq!(released, [4].into_iter().collect());
        assert_eq!(recorded_released, [4].into_iter().collect());

        let have = [0, 0, 0, 0, 10, 0, 0, 0, 31];
        release_changed_noise(&noise, &mut released, &mut recorded_released, &have, &want);
        assert_eq!(released, [4, 8].into_iter().collect());
        assert_eq!(recorded_released, [4].into_iter().collect());
    }
}
