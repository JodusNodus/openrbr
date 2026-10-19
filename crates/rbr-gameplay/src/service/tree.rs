//! The service park's repair tree: every part of the car's `repair.ini` as a node with its drawn times (`M5b16f0`
//! 0x5b16f0, `M0dc0` 0x5b0dc0), the shown estimate (`Ma50` 0x5b0a50), and the unmount and mount steps the repairs run
//! (`M6f0` 0x5b06f0, `M820` 0x5b0820, `Mfbd0` 0x5afbd0, `M620` 0x5b0620).

use rbr_formats::repair::Part as RepairPart;

use crate::crt::CrtRand;

/// One drawn time (0x5b0ea3): `r = rand * (1/32767) * 2`, below or at 1 the time less `r * min`, else more by
/// `(r - 1) * max`.
pub fn draw_time(time: f32, min: f32, max: f32, rand: i32) -> f32 {
    let r = rand as f32 * 3.051_851e-5 * 2.0;
    if r <= 1.0 {
        time - r * min
    } else {
        time + (r - 1.0) * max
    }
}

/// A part's progress (the node's +0x4c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Idle,
    Unmounting,
    Mounting,
    Done,
}

/// One node of the repair tree (the game's 0x54-byte node; `M5b16f0` keeps every part of the file, at any depth, in
/// one array in the file's order, the parent at +0x0 and the children list at +0x50).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: u32,
    /// The file's debug name, a key of the string table (`bstrings`, ignoring case).
    pub name: String,
    pub importance: i32,
    /// The file's unmount and mount times, scaled by the crew (+0x38, +0x3c).
    pub base_unmount: f32,
    pub base_mount: f32,
    /// The drawn unmount and mount times, scaled by the crew (+0x40, +0x44).
    pub unmount: f32,
    pub mount: f32,
    pub progress: Progress,
    /// Seconds into the current step (+0x48).
    pub done: f32,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

/// The tree as `M5b16f0` (0x5b16f0) and `M0dc0` (0x5b0dc0) build it: every part in the file's order (parent before its
/// children), the times drawn in that order, then scaled as the asm multiplies: `A * B * mount`, `A * unmount * B`.
pub(super) fn build(
    parts: &[RepairPart],
    parent: Option<usize>,
    (a, b): (f32, f32),
    rand: &mut CrtRand,
    nodes: &mut Vec<Node>,
) {
    for part in parts {
        let unmount = draw_time(part.unmount.time, part.unmount.min_variance, part.unmount.max_variance, rand.draw());
        let mount = draw_time(part.mount.time, part.mount.min_variance, part.mount.max_variance, rand.draw());
        let at = nodes.len();
        nodes.push(Node {
            id: part.physics_id as u32,
            name: part.name.clone(),
            importance: part.importance,
            base_unmount: a * part.unmount.time * b,
            base_mount: a * b * part.mount.time,
            unmount: a * unmount * b,
            mount: a * b * mount,
            progress: Progress::Idle,
            done: 0.0,
            parent,
            children: Vec::new(),
        });
        if let Some(p) = parent {
            nodes[p].children.push(at);
        }
        build(&part.children, Some(at), (a, b), rand, nodes);
    }
}

/// Every node index of the subtree at `i`, `i` first.
pub(super) fn subtree(nodes: &[Node], i: usize) -> Vec<usize> {
    let mut out = vec![i];
    for &c in &nodes[i].children {
        out.extend(subtree(nodes, c));
    }
    out
}

/// The subtree's shown time (`Ma50` 0x5b0a50): each step at the file's time until it starts, moving to the drawn time
/// with its progress, the drawn time once done.
pub(super) fn shown(nodes: &[Node], i: usize) -> f32 {
    let n = &nodes[i];
    let mount = match n.progress {
        Progress::Idle | Progress::Unmounting => n.base_mount,
        Progress::Mounting if n.mount > 0.0 => (n.mount - n.base_mount) * (n.done / n.mount) + n.base_mount,
        Progress::Mounting => 0.0,
        Progress::Done => n.mount,
    };
    let unmount = match n.progress {
        Progress::Idle | Progress::Unmounting if n.unmount > 0.0 => {
            (n.unmount - n.base_unmount) / n.unmount * n.done + n.base_unmount
        }
        Progress::Idle | Progress::Unmounting => 0.0,
        _ => n.unmount,
    };
    n.children.iter().fold(unmount + mount, |t, &c| shown(nodes, c) + t)
}

/// `M620` (0x5b0620): the subtree's drawn time still to spend, every node then done.
pub(super) fn finish(nodes: &mut [Node], i: usize) -> f32 {
    let n = &nodes[i];
    let own = match n.progress {
        Progress::Idle => n.unmount + n.mount,
        Progress::Unmounting => n.unmount + n.mount - n.done,
        Progress::Mounting => n.mount - n.done,
        Progress::Done => 0.0,
    };
    let left = nodes[i].children.clone().into_iter().fold(own, |t, c| finish(nodes, c) + t);
    nodes[i].progress = Progress::Done;
    nodes[i].done = 0.0;
    left
}

/// A step's time spent (`Mfbd0` and `M6f0`'s own): `done` grows by `dt`, capped at `time`; returns what is left over.
fn spend(done: &mut f32, time: f32, dt: f32) -> f32 {
    *done += dt;
    if *done > time {
        let spare = *done - time;
        *done = time;
        spare
    } else {
        0.0
    }
}

/// `M6f0` (0x5b06f0): unmounts the subtree on the shared `dt`, children first; true when time is left over.
pub(super) fn unmount_step(nodes: &mut [Node], i: usize, dt: &mut f32) -> bool {
    for c in nodes[i].children.clone() {
        if *dt <= 0.0 {
            return false;
        }
        unmount_step(nodes, c, dt);
    }
    if *dt <= 0.0 {
        return false;
    }
    let n = &mut nodes[i];
    match n.progress {
        Progress::Mounting | Progress::Done => return true,
        Progress::Idle => {
            n.progress = Progress::Unmounting;
            n.done = 0.0;
        }
        Progress::Unmounting => {}
    }
    *dt = spend(&mut n.done, n.unmount, *dt);
    *dt > 0.0
}

/// `M820` (0x5b0820): mounts the node back, then its children; false when the time ran out on a step.
pub(super) fn mount_step(nodes: &mut [Node], i: usize, dt: &mut f32) -> bool {
    if *dt > 0.0 {
        let n = &mut nodes[i];
        // The asm compares `done` with the unmount time here and goes on either way. An idle node lands on an `int3`;
        // `M6f0` has unmounted the whole subtree before this runs, so none is.
        if n.progress == Progress::Unmounting {
            n.progress = Progress::Mounting;
            n.done = 0.0;
        }
        if n.progress == Progress::Mounting {
            *dt = spend(&mut n.done, n.mount, *dt);
            if *dt <= 0.0 {
                return false;
            }
            n.progress = Progress::Done;
            n.done = 0.0;
        }
    }
    for c in nodes[i].children.clone() {
        if *dt <= 0.0 {
            return true;
        }
        if !mount_step(nodes, c, dt) {
            return false;
        }
    }
    true
}
