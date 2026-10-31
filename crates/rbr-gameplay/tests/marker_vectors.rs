//! Compares the Rust marker ticker and E25 word maps with original direct-call records from
//! `harness/direct_calls.py --set gameplay_markers`.
//!
//! Set `RBR_MARKER_VECTORS` to one or more colon-separated record paths, or `RBR_VECTOR_STORE` (its
//! `gameplay_markers/`). The records include ordinary pace-note windows on two real stage lists, plus isolated finish
//! and end markers.

use rbr_formats::markers::MarkerKey;
use rbr_formats::trk::{self, ChunkPayload, DriveLineRecord};
use rbr_gameplay::driveline::{dot150, sample_direction};
use rbr_gameplay::markers::{
    flag_word_id, slot_word_id, EndRaceContext, MarkerEvent, MarkerTicker, PaceCallout, TickInput,
};
use std::collections::BTreeMap;

mod common;
use common::hex;

const TICK: u32 = 0x4783b0;
const SLOT_ID: u32 = 0x4b55e0;
const FLAG_ID: u32 = 0x4b5540;
const DOT150: u32 = 0x476450;
const SEEK_D: u32 = 0x476bf0;
const PACE_EMIT: u32 = 0x477860;
const END_RACE_EMIT: u32 = 0x477920;
const FINISH_GATE: u32 = 0x565e80;
const FINISH_MENU: u32 = 0x573530;
const SAMPLE: u32 = 0x4762a0;

#[derive(Default)]
struct Window {
    address: u32,
    pre: Vec<u8>,
    post: Vec<u8>,
}

#[derive(Default)]
struct Inner {
    address: u32,
    eax: u32,
    st0: f64,
    windows: BTreeMap<u32, Window>,
}

#[derive(Default)]
struct Call {
    address: u32,
    args: Vec<u32>,
    eax: u32,
    st0: f64,
    windows: BTreeMap<u32, Window>,
    inner: Vec<Inner>,
}

fn parse(text: &str) -> Vec<Call> {
    let mut calls: Vec<Call> = Vec::new();
    let mut inner_index = None;
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first().copied() {
            Some("call") => {
                inner_index = None;
                calls.push(Call {
                    address: u32::from_str_radix(f[2].trim_start_matches("0x"), 16).unwrap(),
                    ..Call::default()
                });
            }
            Some("args") => {
                calls.last_mut().unwrap().args =
                    f[1..].iter().map(|word| u32::from_str_radix(word, 16).unwrap()).collect();
            }
            Some("win") => {
                let index = f[1].parse().unwrap();
                let window = calls.last_mut().unwrap().windows.entry(index).or_default();
                window.address = u32::from_str_radix(f[2].trim_start_matches("0x"), 16).unwrap();
                if f[4] == "pre" {
                    window.pre = f.get(5).map_or_else(Vec::new, |data| hex(data));
                } else {
                    window.post = f.get(5).map_or_else(Vec::new, |data| hex(data));
                }
            }
            Some("inner") => {
                let inner = Inner {
                    address: u32::from_str_radix(f[1].trim_start_matches("0x"), 16).unwrap(),
                    eax: u32::from_str_radix(f[2], 16).unwrap(),
                    st0: f64::from_bits(u64::from_str_radix(f[3], 16).unwrap()),
                    ..Inner::default()
                };
                let call = calls.last_mut().unwrap();
                call.inner.push(inner);
                inner_index = Some(call.inner.len() - 1);
            }
            Some("iwin") => {
                let index = f[1].parse().unwrap();
                let window = calls.last_mut().unwrap().inner[inner_index.expect("iwin without inner")]
                    .windows
                    .entry(index)
                    .or_default();
                window.address = u32::from_str_radix(f[2].trim_start_matches("0x"), 16).unwrap();
                window.pre = hex(f[4]);
                window.post = hex(f[5]);
            }
            Some("ret") => {
                let call = calls.last_mut().unwrap();
                call.eax = u32::from_str_radix(f[1], 16).unwrap();
                call.st0 = f64::from_bits(u64::from_str_radix(f[2], 16).unwrap());
            }
            _ => {}
        }
    }
    calls
}

fn records() -> Option<Vec<Call>> {
    let mut calls = Vec::new();
    for path in record_paths("RBR_MARKER_VECTORS", "gameplay_markers")? {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        calls.extend(parse(&text));
    }
    Some(calls)
}

/// The record files: those `var` names, or else every `.txt` in `$RBR_VECTOR_STORE/<dir>`; `None` without either.
fn record_paths(var: &str, dir: &str) -> Option<Vec<std::path::PathBuf>> {
    if let Ok(paths) = std::env::var(var) {
        return Some(paths.split(':').map(Into::into).collect());
    }
    let dir = std::path::Path::new(&std::env::var_os("RBR_VECTOR_STORE")?).join(dir);
    let mut paths: Vec<_> = std::fs::read_dir(dir).ok()?.map(|e| e.unwrap().path()).collect();
    paths.retain(|p| p.extension().is_some_and(|x| x == "txt"));
    paths.sort();
    Some(paths)
}

fn tarmac_track_records() -> Option<Vec<DriveLineRecord>> {
    let data = std::env::var_os("RBR_DATA").map(std::path::PathBuf::from)?;
    let path = data.join("Maps/track-41_M.trk");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let file = trk::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Some(
        file.chunks
            .iter()
            .find_map(|chunk| match &chunk.payload {
                ChunkPayload::DriveLine(data) => Some(data.records.clone()),
                _ => None,
            })
            .expect("tarmac .trk has a driveline"),
    )
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn float(bytes: &[u8], at: usize) -> f32 {
    f32::from_bits(word(bytes, at))
}

fn words(bytes: &[u8]) -> Vec<MarkerKey> {
    assert_eq!(bytes.len() % 12, 0);
    bytes
        .chunks_exact(12)
        .map(|key| MarkerKey { tag: word(key, 0), word1: word(key, 4), distance: float(key, 8) })
        .collect()
}

fn callout_from_signal(inner: &Inner) -> PaceCallout {
    let signal = &inner.windows[&0].pre;
    let key = &inner.windows[&1].pre;
    assert_eq!(signal.len(), 12);
    assert_eq!(key.len(), 8);
    let tag = word(key, 0);
    let word1 = word(key, 4);
    PaceCallout {
        x: float(signal, 0),
        key: MarkerKey {
            tag,
            word1,
            // The signal points into the key array, whose distance is compared separately via the ticker state.
            distance: 0.0,
        },
        seek_d: float(signal, 8),
        slot_word_id: slot_word_id(tag),
        flag_word_id: flag_word_id(word1),
    }
}

fn assert_key_eq(got: MarkerKey, want: MarkerKey, where_: &str) {
    assert_eq!(got.tag, want.tag, "{where_}: tag");
    assert_eq!(got.word1, want.word1, "{where_}: word1");
    assert_eq!(got.distance.to_bits(), want.distance.to_bits(), "{where_}: distance");
}

fn assert_callout_eq(got: PaceCallout, want: PaceCallout, where_: &str) {
    assert_eq!(got.x.to_bits(), want.x.to_bits(), "{where_}: x");
    assert_eq!(got.key.tag, want.key.tag, "{where_}: key tag");
    assert_eq!(got.key.word1, want.key.word1, "{where_}: key word1");
    assert_eq!(got.seek_d.to_bits(), want.seek_d.to_bits(), "{where_}: seek_d");
    assert_eq!(got.slot_word_id, want.slot_word_id, "{where_}: slot word");
    assert_eq!(got.flag_word_id, want.flag_word_id, "{where_}: flag word");
}

fn emitted_events(call: &Call) -> Vec<MarkerEvent> {
    let mut events = Vec::new();
    for inner in &call.inner {
        match inner.address {
            PACE_EMIT => events.push(MarkerEvent::PaceCallout(callout_from_signal(inner))),
            END_RACE_EMIT => {
                let payload = &inner.windows[&0].pre;
                assert_eq!(payload.len(), 20);
                events.push(MarkerEvent::EndRace {
                    context: EndRaceContext { words: [0, 4, 8, 12].map(|offset| word(payload, offset)) },
                    state: word(payload, 16),
                });
            }
            FINISH_MENU => events.push(MarkerEvent::FinishMenu),
            _ => {}
        }
    }
    events
}

fn input_for(call: &Call) -> TickInput {
    let seek_d = call.inner.iter().find(|inner| inner.address == SEEK_D).map_or(0.0, |inner| inner.st0 as f32);
    let finish_to_menu =
        call.inner.iter().find(|inner| inner.address == FINISH_GATE).is_some_and(|inner| inner.eax != 0);
    let dot150 =
        call.inner.iter().find(|inner| inner.address == DOT150).expect("ticker record has no Dot150").st0 as f32;
    let w = &call.windows;
    let context = EndRaceContext { words: [5, 7, 8, 9].map(|index| word(&w[&index].pre, 0)) };
    TickInput {
        x: f32::from_bits(call.args[1]),
        dot150,
        seek_d,
        lookahead_scale: float(&w[&2].pre, 0),
        pace_speed_threshold: float(&w[&3].pre, 0),
        finish_to_menu,
        split_host_value: word(&w[&5].pre, 0),
        end_race: context,
        ..TickInput::default()
    }
}

#[test]
fn ticker_matches_original_key_bits_and_emitted_events() {
    let Some(calls) = records() else {
        eprintln!("RBR_MARKER_VECTORS and RBR_VECTOR_STORE unset; skipped");
        return;
    };
    let mut checked = 0;
    let mut end_races = 0;
    let mut finish_menus = 0;
    for (n, call) in calls.iter().filter(|call| call.address == TICK).enumerate() {
        let original_before = words(&call.windows[&1].pre);
        let original_after = words(&call.windows[&1].post);
        let (ticker, events) = MarkerTicker::from_keys(original_before).tick(input_for(call));
        assert_eq!(ticker.keys().len(), original_after.len(), "ticker {n}: key count");
        for (i, (&got, &want)) in ticker.keys().iter().zip(&original_after).enumerate() {
            assert_key_eq(got, want, &format!("ticker {n}, key {i}"));
        }
        let observed = emitted_events(call);
        let expected = events
            .iter()
            .copied()
            .filter(|event| {
                matches!(event, MarkerEvent::PaceCallout(_) | MarkerEvent::EndRace { .. } | MarkerEvent::FinishMenu)
            })
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), observed.len(), "ticker {n}: emitted event count");
        for (index, (want, got)) in expected.iter().zip(&observed).enumerate() {
            match (*want, *got) {
                (MarkerEvent::PaceCallout(want), MarkerEvent::PaceCallout(got)) => {
                    assert_callout_eq(got, want, &format!("ticker {n}, callout {index}"));
                }
                (
                    MarkerEvent::EndRace { context: want_context, state: want_state },
                    MarkerEvent::EndRace { context: got_context, state: got_state },
                ) => {
                    assert_eq!(got_context, want_context, "ticker {n}: EndRace context");
                    assert_eq!(got_state, want_state, "ticker {n}: EndRace state");
                    end_races += 1;
                }
                (MarkerEvent::FinishMenu, MarkerEvent::FinishMenu) => finish_menus += 1,
                _ => panic!("ticker {n}, event {index}: expected {want:?}, got {got:?}"),
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "no Load3b0 records");
    assert!(end_races > 0, "records did not emit EndRace");
    assert!(finish_menus > 0, "records did not take the finish menu path");
    eprintln!("Load3b0: {checked} records equal; {end_races} EndRace and {finish_menus} finish-menu events");
}

#[test]
fn e25_word_maps_match_original_direct_calls() {
    let Some(calls) = records() else {
        eprintln!("RBR_MARKER_VECTORS and RBR_VECTOR_STORE unset; skipped");
        return;
    };
    let mut slot_count = 0;
    let mut flag_count = 0;
    for call in &calls {
        match call.address {
            SLOT_ID => {
                assert_eq!(call.args.len(), 1);
                assert_eq!(slot_word_id(call.args[0]), call.eax as i32, "slot input {:#x}", call.args[0]);
                slot_count += 1;
            }
            FLAG_ID => {
                assert_eq!(call.args.len(), 1);
                assert_eq!(flag_word_id(call.args[0]), call.eax as i32, "flag input {:#x}", call.args[0]);
                flag_count += 1;
            }
            _ => {}
        }
    }
    assert!(slot_count > 0, "no E25_SlotId records");
    assert!(flag_count > 0, "no E25_BitId records");
    eprintln!("E25 maps: {slot_count} slot and {flag_count} flag records equal");
}

#[test]
fn sampler_and_dot150_match_original_direct_calls() {
    let Some(calls) = records() else {
        eprintln!("RBR_MARKER_VECTORS and RBR_VECTOR_STORE unset; skipped");
        return;
    };
    let Some(track) = tarmac_track_records() else {
        eprintln!("RBR_DATA unset; skipped sampler vectors");
        return;
    };
    let mut samples = 0;
    for (index, call) in calls.iter().filter(|call| call.address == SAMPLE).enumerate() {
        let position = float(&call.windows[&1].pre, 0);
        let expected = sample_direction(&track, position).expect("valid .trk sampler input");
        let actual = &call.windows[&0].post;
        assert_eq!(actual.len(), 16);
        for (lane, value) in expected.iter().enumerate() {
            assert_eq!(
                float(actual, lane * 4).to_bits(),
                value.to_bits(),
                "Sample vector {index}, lane {lane}, position {position}"
            );
        }
        samples += 1;
    }
    assert!(samples > 0, "no E12_Sampler::Sample records");

    let mut dots = 0;
    for (index, call) in calls.iter().filter(|call| call.address == DOT150).enumerate() {
        let sampler = &call.windows[&0].pre;
        let position = float(sampler, 0);
        let velocity = [16, 20, 24].map(|offset| float(sampler, offset));
        let expected = dot150(&track, position, velocity).expect("valid .trk Dot150 input");
        // st0 holds an f32 value: the game's x87 runs at 24-bit precision.
        let original = call.st0;
        assert_eq!(
            original.to_bits(),
            f64::from(expected).to_bits(),
            "Dot150 vector {index}, position {position}: original {original}, got {expected}"
        );
        dots += 1;
    }
    assert!(dots > 0, "no E12_Sampler::Dot150 records");
    eprintln!("sampler: {samples} Sample and {dots} Dot150 records equal");
}
