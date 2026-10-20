//! An FMOD bank's mixing tree (`crate::fmod`; ours, standing for FMOD's mixer): each loop and one-shot plays into
//! its track, and each bus runs its effects before the fader ([`Chain`]), its fader gain, its effects after the
//! fader, and adds the result into its parent (sends into their returns), down to the master bus
//! ([`rbr_formats::fmod::Bus`]). The buses are mono (the voices are), at FMOD's scale (a full-scale sample is 1);
//! the master goes out on the car's side.

use std::sync::Arc;

use rbr_formats::fmod::Bus;

use super::dsp::{Chain, Stage};
use super::voices::{balance, Gain, LoopVoice};
use super::FULL_SCALE;
use crate::fmod::{FmodCar, Params};
use crate::voice::Command;

/// How long a bus keeps running after its input stops, seconds (ours: a reverb's tail, a filter's state; what it
/// adds keeps its parent running).
const TAIL_S: f32 = 2.0;

#[derive(Debug, Clone)]
struct Node {
    parent: Option<usize>,
    pre: Chain,
    post: Chain,
    gain: Gain,
    buf: Vec<f32>,
    /// Samples since it last had input.
    idle: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Tree {
    nodes: Vec<Node>,
    /// Every bus after the buses feeding it.
    order: Vec<usize>,
    /// Where the voices of a layer without a track go: the master bus.
    pub(crate) master: usize,
    /// The car's side (-1 left to 1 right).
    pub(crate) pan: f32,
    tail: usize,
}

impl Tree {
    pub(crate) fn new(buses: &[Bus], rate: u32) -> Tree {
        let nodes: Vec<Node> = buses
            .iter()
            .map(|b| Node {
                parent: b.parent,
                pre: Chain::new(&b.pre, rate),
                post: Chain::new(&b.post, rate),
                gain: Gain { now: 1.0, target: 1.0 },
                buf: Vec::new(),
                idle: usize::MAX,
            })
            .collect();
        // Depth-first from each bus to what it feeds, appending a bus once all it feeds is placed, then reversed.
        let feeds = |k: usize| -> Vec<usize> {
            let n = &nodes[k];
            let sends = n.pre.stages.iter().chain(&n.post.stages).filter_map(|s| s.target);
            n.parent.into_iter().chain(sends).filter(|&t| t < nodes.len()).collect()
        };
        let mut state = vec![0u8; nodes.len()];
        let mut order = Vec::new();
        fn visit(k: usize, feeds: &dyn Fn(usize) -> Vec<usize>, state: &mut [u8], order: &mut Vec<usize>) {
            if state[k] != 0 {
                return;
            }
            state[k] = 1;
            for t in feeds(k) {
                visit(t, feeds, state, order);
            }
            state[k] = 2;
            order.push(k);
        }
        for k in 0..nodes.len() {
            visit(k, &feeds, &mut state, &mut order);
        }
        order.reverse();
        let master = buses.iter().position(|b| b.parent.is_none() && b.name == "bus:/").unwrap_or(0);
        Tree { nodes, order, master, pan: 0.0, tail: (TAIL_S * rate as f32) as usize }
    }

    /// Bus `bus`'s input for a block of `len` samples, to add into.
    pub(crate) fn input(&mut self, bus: usize, len: usize) -> Option<&mut [f32]> {
        let n = self.nodes.get_mut(bus)?;
        n.buf.resize(len, 0.0);
        n.idle = 0;
        Some(&mut n.buf)
    }

    pub(crate) fn set_gain(&mut self, bus: usize, gain: f32) {
        if let Some(n) = self.nodes.get_mut(bus) {
            n.gain.target = gain;
        }
    }

    /// Bus `bus`'s effect `effect` (counting those before the fader, then those after) parameter `param`.
    pub(crate) fn set_param(&mut self, bus: usize, effect: usize, param: usize, value: f32, rate: u32) {
        let Some(n) = self.nodes.get_mut(bus) else { return };
        let pre = n.pre.stages.len();
        if effect < pre {
            n.pre.set(effect, param, value, rate);
        } else {
            n.post.set(effect - pre, param, value, rate);
        }
    }

    /// Runs the buses that have input or are still ringing over a block of `out.len()` samples, and adds the master
    /// to `out`.
    pub(crate) fn render(&mut self, out: &mut [[f32; 2]], k: f32, scale: f32) {
        let len = out.len();
        for i in 0..self.order.len() {
            let b = self.order[i];
            let node = &mut self.nodes[b];
            if node.idle > self.tail {
                continue;
            }
            node.buf.resize(len, 0.0);
            let mut buf = std::mem::take(&mut node.buf);
            if (node.gain.target - node.gain.now).abs() < 1e-6 {
                node.gain.now = node.gain.target;
            }
            if node.pre.stages.is_empty() && node.post.stages.is_empty() && node.gain.now == node.gain.target {
                // A plain bus at a steady gain (most tracks).
                let g = node.gain.now;
                if g != 1.0 {
                    buf.iter_mut().for_each(|s| *s *= g);
                }
            } else {
                for s in buf.iter_mut() {
                    let x = node.pre.run(*s) * node.gain.next(k);
                    *s = node.post.run(x);
                }
            }
            node.idle = node.idle.saturating_add(len);
            let parent = node.parent;
            let pre = node.pre.stages.len();
            for j in 0..pre + node.post.stages.len() {
                fn stage(n: &mut Node, j: usize, pre: usize) -> &mut Stage {
                    if j < pre {
                        &mut n.pre.stages[j]
                    } else {
                        &mut n.post.stages[j - pre]
                    }
                }
                let s = stage(&mut self.nodes[b], j, pre);
                let Some(t) = s.target else { continue };
                let mut sent = std::mem::take(&mut s.sent);
                if let Some(into) = self.input(t, len) {
                    into.iter_mut().zip(&sent).for_each(|(a, b)| *a += b);
                }
                sent.clear();
                stage(&mut self.nodes[b], j, pre).sent = sent;
            }
            match parent {
                Some(p) => {
                    if let Some(into) = self.input(p, len) {
                        into.iter_mut().zip(&buf).for_each(|(a, b)| *a += b);
                    }
                }
                None if b == self.master => {
                    let side = balance(self.pan).map(|g| g * scale);
                    for (o, s) in out.iter_mut().zip(&buf) {
                        o[0] += s * side[0];
                        o[1] += s * side[1];
                    }
                }
                None => {}
            }
            buf.fill(0.0);
            self.nodes[b].buf = buf;
        }
    }
}

/// The rate, the settle and the measured time of [`fmod_loudness`] (ours).
const LOUDNESS_RATE: u32 = 24000;
const SETTLE_S: f32 = 0.1;
const MEASURE_S: f32 = 0.15;
/// The operating points [`fmod_loudness`] plays: rpm, throttle, and outside the camera's angle from behind, 6 m away.
const RPMS: [f32; 4] = [2000.0, 3500.0, 5000.0, 6500.0];
const THROTTLES: [f32; 2] = [0.2, 1.0];
const ANGLES: [f32; 3] = [0.0, 90.0, 180.0];
const DISTANCE: f32 = 6.0;

/// `car`'s engine event's level in the car or outside with its voices at `trim` (ours): the RMS, full scale 1, of
/// the event played through the bank's tree at each of [`RPMS`], [`THROTTLES`] and outside [`ANGLES`], the points'
/// energies averaged. The tree's compressors make it more than proportional to `trim`.
pub(crate) fn fmod_loudness(car: &FmodCar, outside: bool, trim: f32) -> f32 {
    let angles: &[f32] = if outside { &ANGLES } else { &[0.0] };
    let distance = if outside { DISTANCE } else { 0.0 };
    let waves: Vec<(Arc<crate::data::Wave>, Option<usize>)> = car.loop_waves();
    let (settle, measure) = ((SETTLE_S * LOUDNESS_RATE as f32) as usize, (MEASURE_S * LOUDNESS_RATE as f32) as usize);
    let mut energy = Vec::new();
    for &rpm in &RPMS {
        for &throttle in &THROTTLES {
            for &cone_angle in angles {
                let params = Params { rpm, throttle, distance, cone_angle, gear: Some(1.0), ..Params::default() };
                let mut tree = Tree::new(&car.buses, LOUDNESS_RATE);
                let mut voices: Vec<(LoopVoice, usize)> = Vec::new();
                for command in car.engine_at(&params, outside) {
                    match command {
                        Command::FmodLoop { slot, gain, pitch, .. } if gain > 0.0 => {
                            let (wave, bus) = &waves[usize::from(slot)];
                            let gain = Gain { now: gain * trim, target: gain * trim };
                            let voice = LoopVoice { wave: wave.clone(), pitch, gain, ..LoopVoice::default() };
                            voices.push((voice, bus.unwrap_or(tree.master)));
                        }
                        Command::FmodBus { bus, gain } => {
                            tree.set_gain(usize::from(bus), gain);
                            if let Some(n) = tree.nodes.get_mut(usize::from(bus)) {
                                n.gain.now = gain;
                            }
                        }
                        Command::FmodEffect { bus, effect, param, value } => {
                            tree.set_param(
                                usize::from(bus),
                                usize::from(effect),
                                usize::from(param),
                                value,
                                LOUDNESS_RATE,
                            );
                        }
                        _ => {}
                    }
                }
                let (mut sum, mut done) = (0.0f64, 0);
                let mut out = vec![[0.0; 2]; 512];
                while done < settle + measure {
                    out.fill([0.0; 2]);
                    for (voice, bus) in &mut voices {
                        if let Some(input) = tree.input(*bus, out.len()) {
                            voice.add_mono(input, LOUDNESS_RATE, 1.0, 1.0 / FULL_SCALE);
                        }
                    }
                    tree.render(&mut out, 1.0, 1.0);
                    for frame in &out {
                        if done >= settle && done < settle + measure {
                            sum += f64::from(frame[0]).powi(2);
                        }
                        done += 1;
                    }
                }
                energy.push(sum / measure as f64);
            }
        }
    }
    (energy.iter().sum::<f64>() / energy.len() as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::fmod::{Effect, DSP_FADER, DSP_SEND};

    fn bus(name: &str, parent: Option<usize>, volume_db: f32, post: Vec<Effect>) -> Bus {
        Bus { name: name.to_owned(), parent, volume_db, automations: Vec::new(), pre: Vec::new(), post }
    }

    #[test]
    fn a_track_runs_its_effects_and_reaches_the_master_and_its_sends_their_return() {
        let fader = Effect { kind: DSP_FADER, params: vec![-6.0], automations: Vec::new(), target: None };
        let send = Effect { kind: DSP_SEND, params: vec![0.0], automations: Vec::new(), target: Some(2) };
        let buses = vec![bus("", Some(1), 0.0, vec![fader, send]), bus("bus:/", None, 0.0, Vec::new()), {
            bus("bus:/return", Some(1), 0.0, Vec::new())
        }];
        let mut tree = Tree::new(&buses, 48000);
        assert_eq!(tree.order.first(), Some(&0));
        assert_eq!(tree.order.last(), Some(&1));
        tree.set_gain(2, 0.5);
        tree.input(0, 4).unwrap().fill(1.0);
        let mut out = [[0.0; 2]; 4];
        tree.render(&mut out, 1.0, 1.0);
        // The track at -6 dB, plus what it sent (after its fader stage) through the return at 0.5 (k = 1: no glide).
        let track = 10f32.powf(-6.0 / 20.0);
        assert!((out[3][0] - track * 1.5).abs() < 1e-5, "{:?}", out);
    }
}
