//! Replays the original's entity calls (`harness/vcu_vectors.py --set entities`) through the ports in
//! `rbr_physics::entities`. Needs record files: `RBR_ENT_VECTORS=<file>[:<file>...]`. Without the
//! variable the tests are skipped.

mod common;

use common::{check_windows, load, map_after_zero, map_windows, shared_windows, Call, Inner};
use rbr_physics::entities::def;
use rbr_physics::entities::exclusion::{self, Heap, ParentTest};
use rbr_physics::entities::frame::{self, Form, Hook, Owner};
use rbr_physics::entities::ground::{self, ProbeCalls, ProbeScratch, Replace};
use rbr_physics::entities::hinge_advance;
use rbr_physics::entities::limit_reflect;
use rbr_physics::entities::transform;
use rbr_physics::entities::tree::{
    integrate, name_walk, walk_gated, walk_pre_order, ChildCall, Children, GatedCalls, Integrate, NameWalk, PreOrder,
};
use rbr_physics::entities::walk::{self, FactoryTable, Node, Walk, Walker};
use rbr_physics::entities::{hinged, layout, material_table};
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";

/// Replays every recorded call of `addr` through `port(mem, args)`, which returns eax. The windows
/// are mapped from the record, the port runs, then eax (when `eax_checked`: the original returns a
/// value) and every window are compared with the original's.
fn replay(addr: u32, name: &str, eax_checked: bool, port: impl Fn(&mut Mem, &[u32]) -> u32) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        // Not reached by the recorded steps (load-time calls are not in them): no evidence here.
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = port(&mut mem, &call.args);
        if eax_checked {
            assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        }
        check_windows(call, &mem, name);
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("{name}: {} calls match, by branch: {branches:?}", calls.len());
}

/// Answers the port's external calls from the recorded ones: each call must be the next recorded one, with the
/// same arguments, and the answer is its eax.
struct Answers<'a> {
    inner: &'a [Inner],
    next: usize,
}

impl Answers<'_> {
    fn answer(&mut self, args: &[u32]) -> u32 {
        let Some(call) = self.inner.get(self.next) else {
            panic!("the port made external call {}, the original made {}", self.next, self.inner.len());
        };
        assert_eq!(call.args, args, "external call {}", self.next);
        self.next += 1;
        call.eax
    }
}

impl Owner for Answers<'_> {
    // Not checked: which slot and form the original called. A record holds only the arguments and the answer,
    // so a port that calls the wrong slot with the same arguments passes.
    fn hook(&mut self, owner: u32, _hook: Hook, _form: Form, a: u32, b: u32) -> u32 {
        self.answer(&[owner, a, b])
    }
    fn gate(&mut self, owner: u32) -> u32 {
        self.answer(&[owner])
    }
    fn frame_of(&mut self, ent: u32) -> u32 {
        self.answer(&[ent])
    }
    fn old_frame_of(&mut self, ent: u32) -> u32 {
        self.answer(&[ent])
    }
}

/// `replay`, for ports that make external calls: `port(mem, args, answers)` returns eax, which must match the
/// original's, and every external call the original recorded must be made.
fn replay_calls(addr: u32, name: &str, port: impl Fn(&mut Mem, &[u32], &mut Answers<'_>) -> u32) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    for call in &calls {
        let mut mem = map_windows(call);
        let mut answers = Answers { inner: &call.inner, next: 0 };
        let eax = port(&mut mem, &call.args, &mut answers);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(answers.next, call.inner.len(), "{name} call {}: external calls", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn entity_frame_hooks_match_original_calls() {
    let hooks = [
        (0x519530, "CEntityFrame::Vf04", Hook::One),
        (0x519560, "CEntityFrame::Vf08", Hook::Two),
        (0x519590, "CEntityFrame::Vf0C", Hook::Three),
        (0x5195C0, "CEntityFrame::Vf10", Hook::Four),
        (0x5195F0, "CEntityFrame::Vf14", Hook::Five),
        (0x519620, "CEntityFrame::Vf18", Hook::Six),
    ];
    for (addr, name, hook) in hooks {
        replay_calls(addr, name, |mem, a, owner| layout::entity_hook_at(mem, a[0], hook, a[1], a[2], owner));
    }
}

#[test]
fn old_entity_frame_hooks_match_original_calls() {
    let hooks = [
        (0x5196D0, "CEntityOldFrame::Vf04", Hook::One),
        (0x519720, "CEntityOldFrame::Vf08", Hook::Two),
        (0x519770, "CEntityOldFrame::Vf0C", Hook::Three),
        (0x5197C0, "CEntityOldFrame::Vf10", Hook::Four),
        (0x519810, "CEntityOldFrame::Vf14", Hook::Five),
        (0x519860, "CEntityOldFrame::Vf18", Hook::Six),
    ];
    for (addr, name, hook) in hooks {
        replay_calls(addr, name, |mem, a, owner| layout::old_hook_at(mem, a[0], hook, a[1], a[2], owner));
    }
}

#[test]
fn e44_mat_table_build_matches_original_calls() {
    replay(0x520190, "E44MatTable::Build", false, |mem, a| {
        layout::build_material_table(mem, a[0], a[1]);
        0
    });
}

#[test]
fn e44_fill_ones_matches_original_calls() {
    replay(0x51CD60, "E44_FillOnes", false, |mem, a| {
        layout::fill_ones(mem, a[0]);
        0
    });
}

#[test]
fn hinged_def_constants_match_original_calls() {
    replay(0x51CC30, "CHingedRigidBody::CDef::Vf4c", true, |_, _| hinged::def_name());
    replay(0x51CC40, "CHingedRigidBody::CDef::Unk20", true, |_, _| hinged::tweak_dict());
}

#[test]
fn hinged_set_650_matches_original_calls() {
    replay(0x51DA50, "CHingedRigidBody::Set650", false, |mem, a| {
        layout::set_hinge_link(mem, a[0], a[1]);
        0
    });
}

#[test]
fn pair_stores_match_original_calls() {
    replay(0x51AD50, "E43PairWalk::MD50", true, |mem, a| layout::store_pair(mem, a[0], a[1], a[2]));
    replay(0x51AD70, "E43PairWalk::MD70", true, |mem, a| layout::store_pair(mem, a[0], a[1], a[2]));
}

#[test]
fn pair_combine_matches_original_calls() {
    replay(0x51AB90, "E33Mat34::Combine", true, |mem, a| layout::store_pair(mem, a[0], a[1], a[2]));
}

#[test]
fn combine_twin_matches_original_calls() {
    replay(0x51AB70, "E33Mat34::CombineTwin", true, |mem, a| layout::store_pair(mem, a[0], a[1], a[2]));
}

/// `E43PairWalk::MFc00` (0x519c00) returns `this`; no record backs it (status `unit`).
#[test]
fn node_of_is_this() {
    assert_eq!(layout::node_of(0x1234_5678), 0x1234_5678);
}

/// `E43PairWalk::MFc10` (0x519c10) returns the old frame (+0x1c), and `CEntity::Vf120` (0x519db0) jumps to the
/// entity's slot 0x11c; the rust_step check covers the original.
#[test]
fn old_frame_and_vf120_target() {
    assert_eq!(layout::old_frame_of(0x1234_5600), 0x1234_561c);
    let mut mem = Mem::new();
    mem.set_u32(0x1000, 0x2000);
    mem.set_u32(0x211c, 0x004e_1200);
    assert_eq!(layout::vf120_target(&mem, 0x1000), 0x004e_1200);
}

/// `CPlainUnaryFunction::Vf04x` and `Vf00x` (0x51a120, 0x51a130) and the tail callback (0x5194a0), against a
/// recording host (no record backs them; status `unit`): the stored callback is called with `this` and the argument,
/// and the callback `E43_Tail40` calls the argument's slot 16 with the argument as `this`.
#[test]
fn functor_calls_callback_or_tail_target() {
    use rbr_physics::entities::functor::{call_stored, Calls, TAIL40};

    struct Host {
        words: Vec<(u32, u32)>,
        calls: Vec<(u32, u32, u32)>,
    }
    impl Calls for Host {
        fn word(&self, at: u32) -> u32 {
            self.words.iter().find(|w| w.0 == at).map_or(0, |w| w.1)
        }
        fn call(&mut self, callee: u32, this: u32, arg: u32) -> u32 {
            self.calls.push((callee, this, arg));
            7
        }
    }

    let (functor, arg, vtable, other) = (0x1000, 0x2000, 0x3000, 0x4000);
    let mut host = Host { words: vec![(functor + 4, other), (arg, vtable), (vtable + 0x40, 0x5000)], calls: vec![] };
    assert_eq!(call_stored(&mut host, functor, arg), 7);
    assert_eq!(host.calls, vec![(other, functor, arg)]);

    host.words[0].1 = TAIL40;
    host.calls.clear();
    assert_eq!(call_stored(&mut host, functor, arg), 7);
    assert_eq!(host.calls, vec![(0x5000, arg, arg)]);
}

#[test]
fn empty_hooks_match_original_calls() {
    replay(0x519B90, "CEntity hook (ret)", false, |_, _| {
        layout::empty_hook();
        0
    });
    replay(0x519AC0, "CEntity hook (xor eax, eax)", true, |_, _| layout::zero_hook());
    replay(0x519A20, "CEntity::E43Vf80 (xor eax, eax)", true, |_, _| layout::zero_hook());
    replay(0x519D60, "CEntity::Vf114 (xor eax, eax)", true, |_, _| layout::zero_hook());
    replay(0x519970, "CEntity::VfE8 (xor eax, eax)", true, |_, _| layout::zero_hook());
    replay(0x519EB0, "CEntity::VfF0 (xor eax, eax)", true, |_, _| layout::zero_hook());
}

#[test]
fn entity_and_pair_echoes_return_first_argument() {
    assert_eq!(frame::entity_result(0x1111, 0x2222, 0x3333), 0x2222);
    assert_eq!(frame::entity_result(0xaaaa, 0xbbbb, 0xcccc), 0xbbbb);
    assert_eq!(walk::return_first(0x1111, 0x2222, 0x3333), 0x2222);
    assert_eq!(walk::return_first(0xaaaa, 0xbbbb, 0xcccc), 0xbbbb);
}

#[test]
fn echo_calls_match_original_calls() {
    replay(0x51A710, "CEntity::E43VfA0", true, |_, a| frame::entity_result(a[0], a[1], a[2]));
    for (addr, name) in [
        (0x51AB10, "E43PairWalk::MAB10"),
        (0x51AB20, "E43PairWalk::MAB20"),
        (0x51AB30, "E43PairWalk::MAB30"),
        (0x51AB40, "E43PairWalk::MAB40"),
    ] {
        replay(addr, name, true, |_, a| walk::return_first(a[0], a[1], a[2]));
    }
}

#[test]
fn hinged_mul4_matches_original_calls() {
    replay(0x51C8C0, "E44Shell::M51C8C0", false, |mem, a| {
        layout::componentwise_mul(mem, a[0], a[1], a[2]);
        0
    });
}

#[test]
fn ground_constants_match_original_calls() {
    replay(0x51BA20, "CGround::CDef::Vf4c", true, |_, _| ground::def_name());
    replay(0x51BB00, "CGround::CDef::Unk20", true, |_, _| ground::tweak_dict());
    for addr in [0x51BB60, 0x51BB70, 0x51BB80] {
        replay(addr, "CGround flag slot", true, |_, _| ground::present() as u32);
    }
    replay(0x51BC70, "CGround::VfCC", true, |_, _| ground::present() as u32);
}

#[test]
fn material_unk20_matches_original_calls() {
    replay(0x520820, "CMaterial::Unk20", true, |_, _| material_table::tweak_dict());
}

#[test]
fn cframe_init_matches_original_calls() {
    replay(0x51AF70, "CFrame::CFrame", false, |mem, a| {
        layout::construct_frame(mem, a[0], a[1]);
        0
    });
}

/// The walk's calls, answered from its record: each hook call and each child walk takes the next inner record,
/// checked against the walker (child walks) and the arguments.
struct WalkAnswers<'a> {
    mem: &'a mut Mem,
    inner: &'a [Inner],
    next: usize,
    /// The bytes a copy must leave at its destination (None: the walk makes no copy).
    copy_expected: Option<&'a [u8]>,
}

impl WalkAnswers<'_> {
    fn take(&mut self) -> &Inner {
        self.next += 1;
        &self.inner[self.next - 1]
    }
}

impl Walk for WalkAnswers<'_> {
    fn node(&self, at: u32) -> Node {
        layout::load_node(self.mem, at)
    }

    fn copy16(&mut self, to: u32, from: u32) {
        layout::copy_mat16(self.mem, to, from);
        if let Some(expected) = self.copy_expected {
            assert_eq!(self.mem.read(to, 16).as_slice(), expected, "copy of {from:#x}");
        }
    }

    fn hook(&mut self, at: u32, hook: Hook, a: u32, b: u32) {
        // The target is not checked: the vtable is not in the record's windows.
        let rec = self.take();
        assert_eq!(rec.args, vec![at, a, b], "{hook:?} of {at:#x}: args");
    }

    fn grid_count(&self, grid: u32) -> u32 {
        layout::grid_count(self.mem, grid)
    }

    fn walk(&mut self, which: Walker, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
        let rec = self.take();
        assert_eq!(rec.addr, which.address(), "{which:?}: walker");
        assert_eq!(rec.args, vec![receiver, o2, a, b], "{which:?}: args");
        rec.eax
    }
}

#[test]
fn pair_walkers_match_original_calls() {
    let walkers = [
        (Walker::PairOneTwo, "E43PairWalk::W9D0"),
        (Walker::PairThreeFour, "E43PairWalk::WA20"),
        (Walker::PairOneTwoDeeper, "E43PairWalk::WFE0"),
        (Walker::PairOneTwoShallower, "E43PairWalk::WB050"),
        (Walker::PairThreeFourDeeper, "E43PairWalk::WB0C0"),
        (Walker::PairThreeFourShallower, "E43PairWalk::WB130"),
        (Walker::RoutePairOneTwo, "E43PairWalk::WB320"),
        (Walker::RoutePairThreeFour, "E43PairWalk::WB370"),
        (Walker::PairFiveSix, "E43PairWalk::WA70"),
        (Walker::PairFiveSixDeeper, "E43PairWalk::WB1A0"),
        (Walker::PairFiveSixShallower, "E43PairWalk::WB210"),
        (Walker::RoutePairFiveSix, "E43PairWalk::WB3C0"),
        (Walker::PairGrids, "E43PairWalk::WAC0"),
        (Walker::PairGridsDeeper, "E43PairWalk::WB280"),
        (Walker::PairGridsShallower, "E43PairWalk::WB2D0"),
        (Walker::RoutePairGrids, "E43PairWalk::WB410"),
    ];
    for (which, name) in walkers {
        let Some(calls) = load(VAR, which.address()) else {
            eprintln!("skipping {name}: {VAR} not set");
            return;
        };
        if calls.is_empty() {
            eprintln!("{name}: no calls in the vector files");
            continue;
        }
        for call in &calls {
            let mut mem = map_windows(call);
            let mut answers = WalkAnswers { mem: &mut mem, inner: &call.inner, next: 0, copy_expected: None };
            let eax = walk::run(&mut answers, which, call.args[0], call.args[1], call.args[2], call.args[3]);
            assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
            assert_eq!(answers.next, call.inner.len(), "{name} call {}: calls", call.index);
            check_windows(call, &mem, name);
        }
        eprintln!("{name}: {} calls match", calls.len());
    }
}

#[test]
fn mat_walk_matches_original_calls() {
    let name = "E43PairWalk::MB560";
    let Some(calls) = load(VAR, 0x51B560) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        // Window 0 is `a` (before the copy), window 1 is `c`: the walk's slot calls write `c` after the copy,
        // and those writes are not modelled, so `c` is checked at the copy only.
        let a_before = &call.windows[0].3;
        let mut answers = WalkAnswers { mem: &mut mem, inner: &call.inner, next: 0, copy_expected: Some(a_before) };
        walk::mb560(&mut answers, call.args[0], call.args[1], call.args[2], call.args[3]);
        assert_eq!(answers.next, call.inner.len(), "{name} call {}: calls", call.index);
        for (k, at, len, _, post) in &call.windows {
            if *k != 1 {
                assert_eq!(mem.read(*at, *len), *post, "{name} call {} window {k}", call.index);
            }
        }
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// The table calls of `lookup_factory_name`, answered from its record: each count, find and advance takes the
/// next inner record and is checked against its target and arguments. `stored` is the index the original's find
/// put in the walker's list (read from the record's after-state): the find's own output is not recorded.
struct FactoryAnswers<'a> {
    mem: &'a mut Mem,
    inner: &'a [Inner],
    next: usize,
    stored: u32,
}

impl FactoryAnswers<'_> {
    fn take(&mut self) -> &Inner {
        self.next += 1;
        &self.inner[self.next - 1]
    }
}

impl FactoryTable for FactoryAnswers<'_> {
    fn count(&mut self) -> u32 {
        let rec = self.take();
        assert_eq!(rec.addr, 0x502050, "count: target");
        assert_eq!(rec.args, vec![0x007C_8040], "count: table");
        rec.eax
    }

    fn find(&mut self, name: u32) -> Option<u32> {
        let rec = self.take();
        assert_eq!(rec.addr, 0x52EBF0, "find: target");
        assert_eq!(rec.args[..2], [0x007C_8040, name], "find: args");
        (rec.eax != 0).then_some(self.stored)
    }

    fn advance(&mut self, cursor: u32) {
        let rec = self.take();
        assert_eq!(rec.addr, 0x52EE50, "advance: target");
        assert_eq!(rec.args, vec![cursor], "advance: cursor");
    }

    fn append_index(&mut self, owner: u32, index: u32) {
        layout::append_name_index(self.mem, owner, index);
    }
}

fn le_word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn factory_name_matches_original_calls() {
    let name = "E43PairWalk::MF8B0";
    let Some(calls) = load(VAR, 0x5198B0) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        // Window 0 is the walker (count at +0x18, indices from +0x1c), before and after the call.
        let (_, _, _, walker_before, walker_after) = &call.windows[0];
        let count_before = le_word(walker_before, 0x18) as usize;
        let stored = le_word(walker_after, 0x1c + 4 * count_before);
        let mut answers = FactoryAnswers { mem: &mut mem, inner: &call.inner, next: 0, stored };
        walk::lookup_factory_name(&mut answers, call.args[0], call.args[1], call.args[3]);
        assert_eq!(answers.next, call.inner.len(), "{name} call {}: calls", call.index);
        for (k, at, len, _, post) in &call.windows {
            // Window 2 is the token cursor: its advance is the table's call, which is not modelled.
            if *k != 2 {
                assert_eq!(mem.read(*at, *len), *post, "{name} call {} window {k}", call.index);
            }
        }
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// The owner of a frame built under the root identity object: the records have no entity, so the constructor
/// makes no owner call, and any call here is a bug in the port.
struct NoOwnerCalls;

impl Owner for NoOwnerCalls {
    fn hook(&mut self, _: u32, _: Hook, _: Form, _: u32, _: u32) -> u32 {
        unreachable!("the root constructor calls no hook")
    }
    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("the root constructor calls no gate")
    }
    fn frame_of(&mut self, _: u32) -> u32 {
        unreachable!("the root constructor asks no entity for its frame")
    }
    fn old_frame_of(&mut self, _: u32) -> u32 {
        unreachable!("the root constructor asks no entity for its old frame")
    }
}

/// `CEntityFrame::CEntityFrame` (0x5194b0) and `CEntityOldFrame::CEntityOldFrame` (0x519650), for the root case
/// (no entity; the frame hangs under the identity object at 0x7cbb00). The entity case is not recorded.
#[test]
fn entity_frame_ctors_match_original_calls() {
    for (addr, name, old) in
        [(0x5194B0, "CEntityFrame::CEntityFrame", false), (0x519650, "CEntityOldFrame::CEntityOldFrame", true)]
    {
        let Some(calls) = load(VAR, addr) else {
            eprintln!("skipping {name}: {VAR} not set");
            continue;
        };
        for call in &calls {
            assert!(call.inner.is_empty(), "{name} call {}: the root case makes no calls", call.index);
            let mut mem = map_windows(call);
            layout::construct_entity_frame(&mut mem, call.args[0], call.args[1], call.args[2], old, &mut NoOwnerCalls);
            check_windows(call, &mem, name);
        }
        eprintln!("{name}: {} calls match", calls.len());
    }
}

/// `CEntity::CEntity` (0x51a640) on the records. The original's inner calls are the two frame constructors and the
/// state's `CBase`; the port makes all three itself (the entity-frame test and the windows check them), so they are
/// checked here by their arguments. The record's arguments are 0 except `options`, so the stores of `definition`,
/// `sort_key` and `parent` are checked only as zeros; the name read and the frame owner calls are not checked.
#[test]
fn entity_ctor_matches_original_calls() {
    let name = "CEntity::CEntity";
    let Some(calls) = load(VAR, 0x51A640) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    for call in &calls {
        let at = call.args[0];
        let args = layout::EntityArgs {
            definition: call.args[1],
            sort_key: call.args[2],
            parent: call.args[3],
            options: call.args[4],
        };
        expect_inner(
            name,
            call,
            &[
                (0x5194B0, vec![at, at, args.parent]),
                (0x519650, vec![at + 0x1C, at, args.parent]),
                (0x52F090, vec![at + 0x38, 0]),
            ],
        );
        let mut mem = map_windows(call);
        // The root identity object's word (0x7cbb00 = 0x0165a2f0, as the frame records show it): the root frame's parent.
        mem.map(0x007C_BB00, &0x0165_A2F0u32.to_le_bytes());
        let eax = layout::construct_entity(&mut mem, at, &args, &mut NoOwnerCalls);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// The parent's test, answered from the record: the one inner call of `ExclusionTest` is the parent's, with the
/// same `other`.
struct ParentAnswers<'a> {
    inner: &'a [Inner],
    next: usize,
}

impl ParentTest for ParentAnswers<'_> {
    fn parent_test(&mut self, parent: u32, other: u32) -> u32 {
        let Some(call) = self.inner.get(self.next) else {
            panic!("the port asked for the parent's test, the original made {} inner calls", self.inner.len());
        };
        assert_eq!(call.args, vec![parent, other], "parent test: args");
        self.next += 1;
        call.eax
    }
}

#[test]
fn exclusion_test_matches_original_calls() {
    let name = "CEntity::E43_ExclF80";
    let Some(calls) = load(VAR, 0x519F80) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    // Branches seen: (eax, the parent was asked, both sets allocated, the same entity).
    let mut branches = std::collections::BTreeMap::<(u32, bool, bool, bool), usize>::new();
    for call in &calls {
        let mem = map_windows(call);
        let mut answers = ParentAnswers { inner: &call.inner, next: 0 };
        let this = layout::load_excluder(&mem, call.args[0]);
        let other = layout::load_excluder(&mem, call.args[1]);
        let eax = exclusion::exclusion_test(&mut answers, &this, &other);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(answers.next, call.inner.len(), "{name} call {}: parent tests", call.index);
        check_windows(call, &mem, name);
        let sets = this.members.is_some() && other.members.is_some();
        *branches.entry((eax, !call.inner.is_empty(), sets, call.args[0] == call.args[1])).or_default() += 1;
    }
    eprintln!("{name}: {} calls match, by (eax, parent asked, sets, same): {branches:?}", calls.len());
}

/// The child walks, answered from their records: the gates, the child calls, the functor calls and the child walks
/// are the walk's inner records, in the port's order, and each is checked against its arguments. The list is read
/// from the walk's own memory (the `nodes` windows).
struct Walks<'a> {
    mem: &'a Mem,
    inner: &'a [Inner],
    next: usize,
}

impl<'a> Walks<'a> {
    fn take(&mut self) -> &'a Inner {
        let Some(rec) = self.inner.get(self.next) else {
            panic!("the port made more inner calls than the original's {}", self.inner.len());
        };
        self.next += 1;
        rec
    }
}

impl Children for Walks<'_> {
    fn first_child(&self, node: u32) -> u32 {
        layout::child_head(self.mem, node)
    }

    fn next_sibling(&self, child: u32) -> u32 {
        layout::sibling(self.mem, child)
    }

    fn gated(&mut self, child: u32) -> bool {
        let rec = self.take();
        assert_eq!(rec.args, vec![child], "gate of {child:#x}");
        rec.eax != 0
    }
}

impl GatedCalls for Walks<'_> {
    fn call(&mut self, child: u32, call: ChildCall) {
        let rec = self.take();
        let args = match call {
            ChildCall::Substep(p) | ChildCall::CarHook(p) | ChildCall::Unknown26(p) | ChildCall::Integrate(p) => {
                vec![child, p]
            }
            _ => vec![child],
        };
        assert_eq!(rec.args, args, "{call:?} of {child:#x}");
    }
}

impl Integrate for Walks<'_> {
    fn name_walk_self(&mut self, node: u32) {
        let rec = self.take();
        assert_eq!(rec.args, vec![node], "name walk of {node:#x}");
    }
}

impl PreOrder for Walks<'_> {
    fn apply_functor(&mut self, functor: u32, child: u32) {
        let rec = self.take();
        assert_eq!(rec.args, vec![functor, child], "functor on {child:#x}");
    }

    fn descend(&mut self, child: u32, functor: u32) {
        let rec = self.take();
        assert_eq!(rec.addr, 0x51A1F0, "descend into {child:#x}: walker");
        assert_eq!(rec.args, vec![child, functor], "descend into {child:#x}: args");
    }
}

impl NameWalk for Walks<'_> {
    fn state_unk44(&mut self, node: u32) {
        let rec = self.take();
        assert_eq!(rec.args, vec![node + 0x38], "state slot 17 of {node:#x}");
    }

    fn walk_children_pre(&mut self, node: u32, _functor: u32) {
        // The functor is the name walk's stack object: its address is not checked (the walk's own record checks it).
        let rec = self.take();
        assert_eq!(rec.addr, 0x51A1F0, "name walk of {node:#x}: walker");
        assert_eq!(rec.args[0], node, "name walk of {node:#x}: node");
    }
}

/// Replays each recorded call of a child walk: `run` makes the port's calls on `walks` and returns nothing (eax is
/// the walk's own return only for ported walks that return one; these return void).
fn replay_walk(addr: u32, name: &str, run: impl Fn(&mut Walks<'_>, &[u32]) -> u32) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    for call in &calls {
        let mem = map_windows(call);
        let mut walks = Walks { mem: &mem, inner: &call.inner, next: 0 };
        run(&mut walks, &call.args);
        assert_eq!(walks.next, call.inner.len(), "{name} call {}: inner calls", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn gated_walks_match_original_calls() {
    // (address, name, the child call for the walk's argument)
    type Gated = fn(u32) -> ChildCall;
    let walks: [(u32, &str, Gated); 7] = [
        (0x51A3A0, "CEntity::E43Vf20", ChildCall::Substep),
        (0x51A3E0, "CEntity::E43Vf28", |_| ChildCall::Unknown10),
        (0x51A440, "CEntity::E43Vf48", |_| ChildCall::Init),
        (0x51A250, "CEntity::E43Vf58", ChildCall::CarHook),
        (0x51A290, "CEntity::E43Vf5C", |_| ChildCall::Force),
        (0x51A2C0, "CEntity::E43Vf64", |_| ChildCall::Torque),
        (0x51A2F0, "CEntity::E43Vf68", ChildCall::Unknown26),
    ];
    for (addr, name, call) in walks {
        replay_walk(addr, name, |w, a| {
            walk_gated(w, a[0], call(a.get(1).copied().unwrap_or(0)));
            0
        });
    }
}

#[test]
fn integrate_walk_matches_original_calls() {
    replay_walk(0x51A330, "CEntity::E43Vf6C", |w, a| {
        integrate(w, a[0], a[1]);
        0
    });
}

#[test]
fn walk_children_pre_matches_original_calls() {
    replay_walk(0x51A1F0, "CEntity::E43_Walk1F0", |w, a| {
        walk_pre_order(w, a[0], a[1]);
        0
    });
}

#[test]
fn name_walk_matches_original_calls() {
    replay_walk(0x51A4B0, "CEntity::Vf40", |w, a| {
        name_walk(w, a[0], 0);
        0
    });
}

#[test]
fn append_child_matches_original_calls() {
    replay(0x51A370, "CEntity::E43Vf150 (AppendChild)", true, |mem, a| layout::append_child(mem, a[0], a[1]));
}

/// The calls an allocation or a def makes on the game, answered from the record: each takes the next inner
/// record, checked against its target and arguments.
struct Calls<'a> {
    inner: &'a [Inner],
    next: usize,
}

impl Calls<'_> {
    fn take(&mut self, addr: u32, args: Vec<u32>) -> u32 {
        let Some(rec) = self.inner.get(self.next) else {
            panic!("the port made more calls than the original's {}", self.inner.len());
        };
        assert_eq!(rec.addr, addr, "call {}: target", self.next);
        assert_eq!(rec.args, args, "call {}: args", self.next);
        self.next += 1;
        rec.eax
    }
}

impl Heap for Calls<'_> {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.take(0x70BEC2, vec![bytes])
    }
}

impl def::Unlink for Calls<'_> {
    fn unlink(&mut self, link: u32) {
        self.take(0x51A180, vec![link]);
    }
}

#[test]
fn alloc_exclusion_matches_original_calls() {
    replay_calls_with(0x51A000, "CEntity::E43_ExclA000 (AllocExclusion)", |mem, a, calls| {
        layout::alloc_exclusion(mem, a[0], a[1], calls)
    });
}

/// Checks the inner records of `call`: the original's calls made by the one call, in order, with their arguments.
fn expect_inner(name: &str, call: &Call, expected: &[(u32, Vec<u32>)]) {
    let got: Vec<(u32, Vec<u32>)> = call.inner.iter().map(|inner| (inner.addr, inner.args.clone())).collect();
    assert_eq!(got, expected, "{name} call {}: inner calls", call.index);
}

/// `E43EntityDef::E43EntityDef` (0x51a530) on the records: its one inner call is the base's constructor, which the
/// port makes itself (`layout::construct_base`), so the windows check the base's writes.
#[test]
fn entity_def_ctor_matches_original_calls() {
    let name = "E43EntityDef::E43EntityDef";
    let Some(calls) = load(VAR, 0x51A530) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = layout::construct_entity_def(&mut mem, call.args[0], call.args[1]);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        expect_inner(name, call, &[(0x52F090, vec![call.args[0], call.args[1]])]);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn entity_def_dtor_matches_original_calls() {
    replay_calls_with(0x51A560, "E43EntityDef::~E43EntityDef", |mem, a, calls| {
        layout::release_entity_def(mem, a[0], calls);
        0
    });
}

/// Replays each recorded call of `addr`: `port` makes the port's calls on `Calls` and returns eax (checked against
/// the record's eax; the inner records must all be made).
fn replay_calls_with(addr: u32, name: &str, port: impl Fn(&mut Mem, &[u32], &mut Calls<'_>) -> u32) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    for call in &calls {
        let mut mem = map_windows(call);
        map_after_zero(&call.after, &mut mem);
        let mut answers = Calls { inner: &call.inner, next: 0 };
        let eax = port(&mut mem, &call.args, &mut answers);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(answers.next, call.inner.len(), "{name} call {}: inner calls", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn fetch_x_matches_original_calls() {
    let name = "E33Mat34::FetchX";
    let Some(calls) = load(VAR, 0x51B6C0) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let t = layout::load_transform(&mem, call.args[0]);
        let (eax, next) = {
            let mut answers = WalkAnswers { mem: &mut mem, inner: &call.inner, next: 0, copy_expected: None };
            let eax = transform::fetch_x(&mut answers, t, call.args[1], call.args[2]);
            (eax, answers.next)
        };
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(next, call.inner.len(), "{name} call {}: walks", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn compose_b_matches_original_calls() {
    let name = "E33Mat34::ComposeB";
    let Some(calls) = load(VAR, 0x51B600) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let [worker, matrix, o2, out] = call.args[..] else { panic!("{name} call {}: args", call.index) };
        let t = transform::Transform { matrix, worker };
        let (eax, next) = {
            let mut answers = WalkAnswers { mem: &mut mem, inner: &call.inner, next: 0, copy_expected: None };
            let eax = transform::compose_b(&mut answers, t, out, o2);
            (eax, answers.next)
        };
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(next, call.inner.len(), "{name} call {}: walks", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// `FetchY`'s grid walk, answered from its record (its writes to `out` are its window).
struct GridAnswers<'a> {
    mem: &'a mut Mem,
    inner: &'a [Inner],
    next: usize,
}

impl transform::GridFetch for GridAnswers<'_> {
    fn copy_grid(&mut self, to: u32, from: u32) {
        layout::copy_grid(self.mem, to, from);
    }

    fn grid_walk(&mut self, receiver: u32, o2: u32, a: u32, b: u32) {
        let rec = &self.inner[self.next];
        self.next += 1;
        assert_eq!(rec.addr, 0x51B410, "grid walk");
        assert_eq!(rec.args, vec![receiver, o2, a, b], "grid walk args");
        for (_, at, _, pre, post) in &rec.windows {
            assert_eq!(self.mem.read(*at, pre.len()), *pre, "grid walk input");
            self.mem.map(*at, post);
        }
    }
}

#[test]
fn fetch_y_matches_original_calls() {
    let name = "E33Mat34::FetchY";
    let Some(calls) = load(VAR, 0x51B780) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    for call in &calls {
        let mut mem = map_windows(call);
        let t = layout::load_transform(&mem, call.args[0]);
        let mut answers = GridAnswers { mem: &mut mem, inner: &call.inner, next: 0 };
        let eax = transform::fetch_y(&mut answers, t, call.args[1], call.args[2]);
        let next = answers.next;
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(next, call.inner.len(), "{name} call {}: walks", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// `CBrakeWall::VfF0`'s slot 0xe8, answered from its record (its output vectors are its window).
struct SlotE8Answers<'a> {
    mem: Mem,
    inner: &'a [Inner],
    next: usize,
}

impl rbr_physics::collision::wall::VfF0Host for SlotE8Answers<'_> {
    fn vf_e8(&mut self, _sp: u32, wall: u32, a: u32, b: u32, out: u32) -> u32 {
        let rec = &self.inner[self.next];
        self.next += 1;
        assert_eq!(rec.args, vec![wall, a, b, 0, out, 1], "VfE8 args");
        for (_, at, _, _, post) in &rec.windows {
            self.mem.map(*at, post);
        }
        rec.eax
    }
}

/// `CBrakeWall::VfE8`'s two `FetchX` calls, answered from its record (the fetched point is its window).
struct FetchXAnswers<'a> {
    mem: Mem,
    inner: &'a [Inner],
    next: usize,
}

impl rbr_physics::collision::wall::VfE8Host for FetchXAnswers<'_> {
    fn fetch_x(&mut self, _sp: u32, matrix: u32, out: u32, worker: u32) {
        let rec = &self.inner[self.next];
        self.next += 1;
        assert_eq!(rec.args, vec![matrix, out, worker], "FetchX args");
        for (_, at, _, _, post) in &rec.windows {
            self.mem.map(*at, post);
        }
    }
}

#[test]
fn brake_wall_vf_e8_matches_original_calls() {
    let name = "CBrakeWall::VfE8";
    let Some(calls) = load(VAR, 0x4E3A10) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    let mut hits = 0;
    for call in &calls {
        let mut mem = common::shared_windows(call);
        // The first point is the frame's +0x60: the entry esp follows from it (the frame is `((esp - 4) & !0xf) - 0x80`).
        let entry = call.inner.first().map_or(0x1000, |c| c.args[1] - 0x60 + 0x80 + 4);
        let mut answers = FetchXAnswers { mem: mem.clone(), inner: &call.inner, next: 0 };
        let mut args = [0; 5];
        args.copy_from_slice(&call.args[1..6]);
        let eax = rbr_physics::collision::layout::brake_wall_vf_e8(&mut answers, &mut mem, call.args[0], args, entry);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(answers.next, call.inner.len(), "{name} call {}: calls", call.index);
        check_windows(call, &mem, name);
        hits += eax;
    }
    eprintln!("{name}: {} calls match, {hits} hits", calls.len());
}

#[test]
fn brake_wall_vf_f0_matches_original_calls() {
    let name = "CBrakeWall::VfF0";
    let Some(calls) = load(VAR, 0x4E3970) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    let mut hits = 0;
    for call in &calls {
        let mut mem = common::shared_windows(call);
        // VfE8's `out` is the frame's +0xc: the entry esp follows from it (the frame is `((esp - 4) & !0xf) - 0x4c`).
        let out = call.inner[0].args[4];
        let entry = out - 0xc + 0x4c + 4;
        let mut answers = SlotE8Answers { mem: mem.clone(), inner: &call.inner, next: 0 };
        let mut args = [0; 8];
        args.copy_from_slice(&call.args[1..9]);
        let eax = rbr_physics::collision::layout::brake_wall_vf_f0(&mut answers, &mut mem, call.args[0], args, entry);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!(answers.next, call.inner.len(), "{name} call {}: calls", call.index);
        check_windows(call, &mem, name);
        hits += eax;
    }
    eprintln!("{name}: {} calls match, {hits} hits", calls.len());
}

#[test]
fn ground_def_ctor_matches_original_calls() {
    replay(0x51B990, "CGround::CDef::CDef", true, |mem, a| {
        layout::construct_ground_def(mem, a[0], a[1]);
        a[0]
    });
}

#[test]
fn ground_def_new_matches_original_calls() {
    replay_calls_with(0x51BAA0, "GroundDefNew9", |mem, a, calls| layout::ground_def_new(mem, a[0], calls));
}

/// `VfE8`'s calls, answered from the record: `FetchX` (the walk is its own record, so this answers the call
/// with its arguments and eax) and the host's query.
struct ProbeAnswers<'a, 'b> {
    calls: &'a mut Calls<'b>,
}

impl ProbeCalls for ProbeAnswers<'_, '_> {
    fn fetch_x(&mut self, this: u32, out: u32, arg: u32) -> u32 {
        self.calls.take(0x51B6C0, vec![this, out, arg])
    }

    fn query(&mut self, host: u32, t60: u32, t70: u32, out: u32) -> u32 {
        self.calls.take(0x4FE810, vec![host, t60, t70, out])
    }

    // The hit path only: no record reaches it (the drive's queries all miss), so the port must not call it here.
    fn copy_from(&mut self, to: u32, from: u32) {
        panic!("CopyFrom({to:#x}, {from:#x}): no recorded call");
    }
}

#[test]
fn ground_box_probe_matches_original_calls() {
    replay_calls_with(0x51C6F0, "CGround::VfE8", |mem, a, calls| {
        // The stack scratch: the matrices FetchX wrote (the first two inner calls take them as `out`) and the
        // host's output (the query's `out`).
        let scratch =
            ProbeScratch { t60: calls.inner[0].args[1], t70: calls.inner[1].args[1], out: calls.inner[2].args[3] };
        // The host's output is its inner window's after bytes: the port reads them after the query, so they are
        // in memory from the start (the query itself is answered from its record).
        for (_, at, _, _, post) in &calls.inner[2].windows {
            mem.map(*at, post);
        }
        let mut answers = ProbeAnswers { calls };
        layout::box_probe(mem, a[0], a[1], a[2], a[4], scratch, &mut answers)
    });
}

/// `CDef::Vf50`'s deleting call on the old ground, and the allocations, for the branches no record reaches.
#[derive(Default)]
struct Retired {
    deleted: Vec<u32>,
    owner_calls: Vec<u32>,
}

impl Replace for Retired {
    fn delete(&mut self, ground: u32) {
        self.deleted.push(ground);
    }
}

impl Owner for Retired {
    fn hook(&mut self, owner: u32, _hook: Hook, _form: Form, _a: u32, _b: u32) -> u32 {
        self.owner_calls.push(owner);
        0
    }
    fn gate(&mut self, _owner: u32) -> u32 {
        0
    }
    fn frame_of(&mut self, ent: u32) -> u32 {
        self.owner_calls.push(ent);
        0
    }
    fn old_frame_of(&mut self, ent: u32) -> u32 {
        self.owner_calls.push(ent);
        0
    }
}

/// A heap that hands out one fixed block (0 for none).
struct OneBlock(u32);

impl Heap for OneBlock {
    fn allocate(&mut self, _bytes: u32) -> u32 {
        self.0
    }
}

#[test]
fn ground_make_replaces_the_singleton() {
    let (def, a, b) = (0x4000, 0x11, 0);
    let mut mem = Mem::new();
    mem.set_u32(0x0165_9268, 0x500);
    mem.set_u32(0x007C_BB00, 0x7000);
    mem.set_u32(def + 4, 0x6000);
    let mut calls = Retired::default();
    let at = layout::ground_make(&mut mem, def, a, b, &mut OneBlock(0x3000), &mut calls);
    assert_eq!(at, 0x3000);
    assert_eq!(calls.deleted, vec![0x500], "the old ground is deleted");
    assert_eq!(mem.u32(0x0165_9268), 0x3000, "the singleton is the new ground");
    assert_eq!(mem.u32(0x3000), ground::GROUND_VTABLE);
    assert_eq!(mem.u32(0x3000 + 0x34), def, "the definition");
    assert_eq!(mem.u32(0x3000 + 0x30), a, "the sort key");
    assert_eq!(mem.u32(0x3000 + 0x60), ground::GROUND_OPTIONS | 1, "the options");
    assert!(calls.owner_calls.is_empty(), "no parent, no frame call");
}

#[test]
fn ground_make_without_memory_clears_the_singleton() {
    let mut mem = Mem::new();
    mem.set_u32(0x0165_9268, 0x500);
    mem.set_u32(0x007C_BB00, 0x7000);
    let mut calls = Retired::default();
    let at = layout::ground_make(&mut mem, 0x4000, 0, 0, &mut OneBlock(0), &mut calls);
    assert_eq!(at, 0);
    assert_eq!(mem.u32(0x0165_9268), 0, "the old ground is deleted and not replaced");
    assert_eq!(calls.deleted, vec![0x500]);
}

/// The hit path of `VfE8` (no record reaches it): the host's floats are the test's (1.0 to 13.0 at `out`), the query
/// hits, and the record's scatter is checked against the asm's stores (`d+0x10..d+0x38`).
struct Hit {
    query_args: Option<Vec<u32>>,
}

impl ProbeCalls for Hit {
    fn fetch_x(&mut self, _this: u32, out: u32, _arg: u32) -> u32 {
        out
    }
    fn query(&mut self, host: u32, t60: u32, t70: u32, out: u32) -> u32 {
        self.query_args = Some(vec![host, t60, t70, out]);
        1
    }
    fn copy_from(&mut self, _to: u32, _from: u32) {}
}

#[test]
fn ground_box_probe_hit_scatters_the_host_output() {
    let (ground, d, out) = (0x5000, 0x8000, 0x9000);
    let mut mem = Mem::new();
    mem.set_u32(0x007C_BB00, 0x7000); // the identity object
    mem.set_u32(0x007C_755C, 0x6000); // the host
    let floats: Vec<f32> = (1..=13).map(|i| i as f32).collect();
    for (i, v) in floats.iter().enumerate() {
        mem.set_f32(out + 4 * i as u32, *v);
    }
    let scratch = ProbeScratch { t60: 0x9100, t70: 0x9200, out };
    let mut calls = Hit { query_args: None };
    let eax = layout::box_probe(&mut mem, ground, 0xA000, 0xA100, d, scratch, &mut calls);
    assert_eq!(eax, 1);
    assert_eq!(calls.query_args, Some(vec![0x6000, 0x9100, 0x9200, out]));
    let word = |k: u32| f32::from_bits(mem.u32(d + 4 * k));
    assert_eq!([word(4), word(5), word(6), word(7)], [5.0, 6.0, 7.0, 8.0], "o[4..8] = out[4..8]");
    assert_eq!([word(8), word(9), word(10)], [11.0, 12.0, 10.0], "o[8..11] = out[10], out[11], out[9]");
    assert_eq!([mem.u32(d + 0x2C), mem.u32(d + 0x30)], [0, 0], "o[11], o[12] cleared");
    assert_eq!(word(13), 13.0, "o[13] = out[12]");
    assert_eq!(mem.u32(d + 0x38), ground, "o[14] = the ground");
}

/// `copy_blocks_then_walk` (0x51b460): each block is copied to its own place, then the walk gets `dst` and `n`.
/// Reached by the drive (no records): the rust_step check covers the original.
#[test]
fn copy_blocks_then_walk_copies_each_block_and_walks() {
    struct Log {
        mem: Mem,
        walks: Vec<(Walker, u32, u32, u32, u32)>,
    }

    impl Walk for Log {
        fn node(&self, _: u32) -> Node {
            unreachable!("the copy walks no node")
        }
        fn hook(&mut self, _: u32, _: Hook, _: u32, _: u32) {
            unreachable!("the copy calls no hook")
        }
        fn grid_count(&self, _: u32) -> u32 {
            unreachable!("the copy walks no grid")
        }
        fn walk(&mut self, which: Walker, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
            self.walks.push((which, receiver, o2, a, b));
            a
        }
        fn copy16(&mut self, to: u32, from: u32) {
            for k in 0..4 {
                let word = self.mem.u32(from + 4 * k);
                self.mem.set_u32(to + 4 * k, word);
            }
        }
    }

    let mut mem = Mem::new();
    for k in 0..8 {
        mem.set_u32(0x1000 + 4 * k, 0x100 + k);
    }
    let mut log = Log { mem, walks: vec![] };
    let dst = transform::copy_blocks_then_walk(&mut log, 0x3000, 0x4000, 0x1000, 0x2000, 2);
    assert_eq!(dst, 0x2000);
    for k in 0..8 {
        assert_eq!(log.mem.u32(0x2000 + 4 * k), 0x100 + k);
    }
    assert_eq!(log.walks, vec![(Walker::RoutePairOneTwo, 0x3000, 0x4000, 0x2000, 2)]);
    // The twin (0x51b500) walks with `RoutePairThreeFour`.
    log.walks.clear();
    let dst = transform::copy_blocks_then_walk_three_four(&mut log, 0x3000, 0x4000, 0x1000, 0x2000, 2);
    assert_eq!(dst, 0x2000);
    assert_eq!(log.walks, vec![(Walker::RoutePairThreeFour, 0x3000, 0x4000, 0x2000, 2)]);
}

/// `limit_update_at` (0x51f280): a value past the first bound copies window one and moves the residual by the tweak's
/// factor after the flag call. The rust_step check covers the original (reached by the drive; no records).
#[test]
fn limit_update_copies_the_window_and_moves_the_residual() {
    #[derive(Default)]
    struct Flags(Vec<u32>);

    impl limit_reflect::Calls for Flags {
        fn flag(&mut self) {
            self.0.push(1);
        }
    }

    let (this, tweak) = (0x1000, 0x2000);
    let mut mem = Mem::new();
    mem.set_u32(this + 0x34, tweak);
    mem.set_f32(this + 0x3fc, 3.0);
    mem.set_f32(this + 0x40c, 0.5);
    for k in 0..3 {
        mem.set_u32(tweak + 0x474 + 4 * k, 0x100 + k);
    }
    mem.set_f32(tweak + 0x47c, 1.0);
    mem.set_f32(tweak + 0x48c, -1.0);
    mem.set_f32(tweak + 0x484, 0.0);
    mem.set_f32(tweak + 0x488, 0.0);
    mem.set_f32(tweak + 0x4ac, 2.0);
    let mut flags = Flags::default();
    let eax = layout::limit_update_at(&mut mem, this, &mut flags);
    assert_eq!(eax, 1.0f32.to_bits());
    assert_eq!(mem.u32(this + 0x3f4), 0x100);
    assert_eq!(mem.f32(this + 0x40c), -1.0);
    assert_eq!(flags.0, vec![1]);
}

/// `sim_f950_at` (0x51f950): a moving body keeps the advanced rate and angle, makes its calls in the original's order and
/// writes the state payload; a settled body (its link's flag set) and a body with no owner car make one call or none.
/// Reached by the drive (no records): the rust_step check covers the original.
#[test]
fn sim_f950_runs_the_calls_in_order_and_writes_the_payload() {
    #[derive(Default)]
    struct Log(Vec<String>);

    impl hinge_advance::Calls for Log {
        fn integrate(&mut self, this: u32, dt: f32) -> u32 {
            self.0.push(format!("integrate {this:#x} {dt}"));
            7
        }

        fn set_dirty(&mut self, at: u32) {
            self.0.push(format!("set_dirty {at:#x}"));
        }

        fn scale(&mut self, vec: u32, s: f32, out: u32) {
            self.0.push(format!("scale {vec:#x} {s} {out:#x}"));
        }

        fn limit_update(&mut self, this: u32) {
            self.0.push(format!("limit_update {this:#x}"));
        }

        fn frame(&mut self, this: u32) {
            self.0.push(format!("frame {this:#x}"));
        }

        fn emit(&mut self, signal: u32, sig: u32) -> u32 {
            self.0.push(format!("emit {signal:#x} {sig:#x}"));
            0x55
        }
    }

    let (this, link, sig) = (0x1000, 0x1700, 0x3000);
    let mut mem = Mem::new();
    mem.map(this, &[0; 0x1000]);
    mem.map(link, &[0; 4]);
    mem.map(sig, &[0; 0x40]);
    mem.map(0x893600, &[0; 4]);
    mem.set_u32(this + 0x34, 0x2000);
    mem.set_u32(this + 0x3d0, 0x4000);
    mem.set_u32(this + 0x650, link);
    mem.set_f32(this + 0x624, 0.5);
    mem.set_f32(this + 0x40c, 1.0);
    mem.set_f32(this + 0x3fc, 3.0);
    mem.set_u32(this + 0x30, 7);
    for k in 0..4 {
        mem.set_f32(this + 0x100 + 4 * k, 1.0 + k as f32);
        mem.set_f32(this + 0x140 + 4 * k, 5.0 + k as f32);
        mem.set_f32(this + 0x1c0 + 4 * k, 9.0);
    }
    let mut log = Log::default();
    let eax = layout::sim_f950_at(&mut mem, this, 0.25, sig, &mut log);
    assert_eq!(eax, 0x55);
    assert_eq!(mem.f32(this + 0x40c), 1.125);
    assert_eq!(mem.f32(this + 0x3fc), 1.125 * 0.25 + 3.0);
    assert_eq!(mem.f32(this + 0x1c0), 0.0);
    assert_eq!(mem.u32(sig), 7);
    assert_eq!(mem.f32(sig + 0x14), 5.0);
    assert_eq!(mem.f32(sig + 0x20), 8.0, "the position's last lane overwrites the rotation's first");
    assert_eq!(mem.f32(sig + 0x24), 2.0);
    assert_eq!(mem.f32(sig + 0x2c), 4.0);
    assert_eq!(
        log.0,
        [
            "set_dirty 0x1400",
            "scale 0x22c0 1.125 0x1190",
            "set_dirty 0x13f0",
            "limit_update 0x1000",
            "frame 0x1000",
            "emit 0xe438 0x3000",
        ]
    );

    mem.set_u32(link, 1);
    let mut log = Log::default();
    assert_eq!(layout::sim_f950_at(&mut mem, this, 0.25, sig, &mut log), link);
    assert!(log.0.is_empty());

    mem.set_u32(link, 0);
    mem.set_u32(this + 0x3d0, 0);
    let mut log = Log::default();
    assert_eq!(layout::sim_f950_at(&mut mem, this, 0.25, sig, &mut log), 7);
    assert_eq!(log.0, ["integrate 0x1000 0.25"]);
}

/// `CEntity::VfCC` (0x519e90) answers 1 and `VfD0` (0x519ea0) answers 0 whatever the arguments: unit checks.
#[test]
fn entity_flag_slots_unit() {
    assert_eq!(layout::vf_cc(), 1);
    assert_eq!(layout::vf_d0(), 0);
}

struct E90BuildRecorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    next: usize,
    frame: u32,
    build_posts: usize,
}

impl E90BuildRecorded<'_> {
    fn take(&mut self, addr: u32, args: &[u32]) -> u32 {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("E90 Build: inner {} is missing", self.next));
        assert_eq!(call.addr, addr, "E90 Build inner {} target", self.next);
        assert_eq!(call.args, args, "E90 Build inner {} args", self.next);
        for (_, at, _, pre, post) in &call.windows {
            self.mem.map(*at, pre);
            self.mem.map(*at, post);
        }
        self.next += 1;
        call.eax
    }
}

impl layout::E90BuildCalls for E90BuildRecorded<'_> {
    fn allocate(&mut self, sp: u32, size: u32) -> u32 {
        assert_eq!(sp, self.frame - 0x28);
        self.take(0x70bec2, &[size])
    }

    fn init(&mut self, sp: u32, pool: u32, count: u32, mem: u32) -> u32 {
        assert_eq!(sp, self.frame - 0x2c);
        self.take(0x601270, &[pool, count, mem])
    }

    fn build_post(&mut self, sp: u32, handler: u32, segment: u32, dummies: u32, mem: u32) -> u32 {
        let depth = if self.build_posts == 0 { 0x30 } else { 0x34 };
        assert_eq!(sp, self.frame - depth);
        self.build_posts += 1;
        self.take(0x602fe0, &[handler, segment, dummies, mem])
    }

    fn build_span(&mut self, sp: u32, handler: u32, span: u32, prev: u32, post: u32, dummy: u32, mem: u32) -> u32 {
        assert_eq!(sp, self.frame - 0x40);
        self.take(0x6029b0, &[handler, span, prev, post, dummy, mem])
    }
}

#[test]
fn e90_group_build_matches_original_calls() {
    let Some(calls) = load(VAR, 0x601470) else {
        eprintln!("skipping E90Group::Build: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        map_after_zero(&call.after, &mut mem);
        let mut host =
            E90BuildRecorded { inner: &call.inner, mem: mem.clone(), next: 0, frame: 0x300000, build_posts: 0 };
        layout::build_e90_group(&mut mem, call.args[0], 0x300000, &mut host);
        assert_eq!(host.next, call.inner.len(), "E90 Build call {}: external calls", call.index);
        check_windows(call, &mem, "E90Group::Build");
    }
    eprintln!("E90Group::Build: {} calls match", calls.len());
}

struct E90SegmentRecorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    next: usize,
    frame: u32,
}

impl E90SegmentRecorded<'_> {
    fn take(&mut self, sp: u32, depth: u32, addr: u32, args: &[u32]) -> u32 {
        assert_eq!(sp, self.frame - depth, "E90 segment call {addr:#x} stack");
        let call = self
            .inner
            .get(self.next)
            .unwrap_or_else(|| panic!("E90 segment inner {} is missing ({addr:#x})", self.next));
        assert_eq!(call.addr, addr, "E90 segment inner {} target", self.next);
        assert_eq!(call.args, args, "E90 segment inner {} args", self.next);
        for (_, at, len, pre, _) in &call.windows {
            assert_eq!(self.mem.read(*at, *len), *pre, "E90 segment inner {} pre", self.next);
        }
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        self.next += 1;
        call.eax
    }
}

impl layout::E90SegmentCalls for E90SegmentRecorded<'_> {
    fn init(&mut self, sp: u32, segment: u32, dummy: u32) {
        let depth = if sp == self.frame - 0xa4 { 0xa4 } else { 0x3c };
        self.take(sp, depth, 0x605b60, &[segment, dummy]);
    }
    fn parts_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.take(sp, if sp == self.frame - 0xa8 { 0xa8 } else { 0x40 }, 0x6026a0, &[segment, count, mem])
    }
    fn springs_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.take(sp, if sp == self.frame - 0xa8 { 0xa8 } else { 0x40 }, 0x6027a0, &[segment, count, mem])
    }
    fn bends_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.take(sp, 0x40, 0x6028b0, &[segment, count, mem])
    }
    fn pins_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.take(sp, 0xa8, 0x602930, &[segment, count, mem])
    }
    fn add_particle(&mut self, sp: u32, segment: u32, pos: u32, inv_mass: f32) -> u32 {
        self.take(sp, if sp == self.frame - 0xa8 { 0xa8 } else { 0x40 }, 0x605f30, &[segment, pos, inv_mass.to_bits()])
    }
    fn add_pin(&mut self, sp: u32, segment: u32, particle: u32) {
        self.take(sp, if sp == self.frame - 0xa4 { 0xa4 } else { 0x3c }, 0x606090, &[segment, particle]);
    }
    fn add_spring(&mut self, sp: u32, segment: u32, a: u32, b: u32, kind: u32, stiffness: f32) -> u32 {
        let depth = if sp == self.frame - 0xb0 { 0xb0 } else { 0x48 };
        self.take(sp, depth, 0x605fb0, &[segment, a, b, kind, stiffness.to_bits()])
    }
    fn add_fixed(&mut self, sp: u32, segment: u32, spring: u32) {
        self.take(sp, 0x3c, 0x606070, &[segment, spring]);
    }
    fn add_bend(&mut self, sp: u32, segment: u32, spring: u32, particle: u32) {
        self.take(sp, 0x40, 0x6060b0, &[segment, spring, particle]);
    }
}

#[test]
fn e90_segment_helpers_match_direct_records() {
    if let Some(calls) = load(VAR, 0x601270) {
        let mut owned = 0;
        for call in &calls {
            let mut mem = shared_windows(call);
            map_after_zero(&call.after, &mut mem);
            let mut host = E90SegPoolRecorded { inner: &call.inner, mem: mem.clone(), next: 0, index: call.index };
            let eax =
                layout::init_e90_seg_pool(&mut mem, call.args[0], call.args[1], call.args[2], 0x300000, &mut host);
            assert_eq!(eax, call.eax, "E90SegPool::Init call {} eax", call.index);
            assert_eq!(host.next, call.inner.len(), "E90SegPool::Init call {} CRT calls", call.index);
            check_windows(call, &mem, "E90SegPool::Init");
            owned += usize::from(call.inner.iter().any(|i| i.addr == 0x70c408));
        }
        eprintln!("E90SegPool::Init: {} calls match, {owned} owned arrays", calls.len());
    }
    if let Some(calls) = load(VAR, 0x600ff0) {
        for call in &calls {
            let mut mem = map_windows(call);
            assert_eq!(
                layout::construct_e90_seg(&mut mem, call.args[0]),
                call.eax,
                "E90Seg::E90Seg call {}",
                call.index
            );
            check_windows(call, &mem, "E90Seg::E90Seg");
        }
        eprintln!("E90Seg::E90Seg: {} calls match", calls.len());
    }
    if let Some(calls) = load(VAR, 0x601030) {
        for call in &calls {
            let mut mem = shared_windows(call);
            let mut host = E90SegPoolRecorded { inner: &call.inner, mem: mem.clone(), next: 0, index: call.index };
            layout::destroy_e90_seg(&mut mem, call.args[0], 0x300000, &mut host);
            assert_eq!(host.next, call.inner.len(), "E90Seg::~E90Seg call {} deletes", call.index);
            check_windows(call, &mem, "E90Seg::~E90Seg");
        }
        eprintln!("E90Seg::~E90Seg: {} calls match", calls.len());
    }
    for (addr, is_span) in [(0x602fe0, false), (0x6029b0, true)] {
        let Some(calls) = load(VAR, addr) else {
            continue;
        };
        for call in &calls {
            let mut mem = shared_windows(call);
            let frame = call.inner.iter().find(|i| i.addr == 0x605f30).unwrap().args[1] + 0x18;
            let mut host = E90SegmentRecorded { inner: &call.inner, mem: mem.clone(), next: 0, frame };
            let eax = if is_span {
                layout::build_e90_span(
                    &mut mem,
                    call.args[0],
                    call.args[1],
                    call.args[2],
                    call.args[3],
                    call.args[4],
                    call.args[5],
                    frame,
                    &mut host,
                )
            } else {
                layout::build_e90_post(
                    &mut mem,
                    call.args[0],
                    call.args[1],
                    call.args[2],
                    call.args[3],
                    frame,
                    &mut host,
                )
            };
            assert_eq!(eax, call.eax, "E90 helper {addr:#x} call {} eax", call.index);
            assert_eq!(host.next, call.inner.len(), "E90 helper {addr:#x} call {} children", call.index);
            check_windows(call, &mem, "E90 segment helper");
        }
    }
}

/// `E90SegPool::Init`'s and `~E90Seg`'s CRT calls, from the record in order: `operator new` returns its eax, the vector
/// iterators run the segment ports (the destructors from the last segment), `operator delete` is checked.
struct E90SegPoolRecorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    next: usize,
    index: usize,
}

impl E90SegPoolRecorded<'_> {
    fn take(&mut self, addr: u32, args: &[u32]) -> u32 {
        let inner = &self.inner[self.next];
        assert_eq!(
            (inner.addr, &inner.args[..]),
            (addr, args),
            "E90 segment call {}: CRT call {}",
            self.index,
            self.next
        );
        self.next += 1;
        inner.eax
    }
}

impl layout::E90SegPoolCalls for E90SegPoolRecorded<'_> {
    fn allocate(&mut self, _sp: u32, size: u32) -> u32 {
        self.take(0x70bec2, &[size])
    }

    fn construct_all(&mut self, _sp: u32, segments: u32, count: u32) {
        for i in 0..count {
            layout::construct_e90_seg(&mut self.mem, segments + i * 0x78);
        }
        self.take(0x70c408, &[segments, 0x78, count, 0x600ff0, 0x601030]);
    }

    fn destroy_all(&mut self, _sp: u32, segments: u32, count: u32) {
        let mut mem = self.mem.clone();
        for i in (0..count).rev() {
            layout::destroy_e90_seg(&mut mem, segments + i * 0x78, 0x300000, self);
        }
        self.take(0x70c4da, &[segments, 0x78, count, 0x601030]);
    }

    fn release(&mut self, _sp: u32, block: u32) {
        self.take(0x70c260, &[block]);
    }
}

#[test]
fn clear480_and_exclusion_remove_match_direct_records() {
    if let Some(calls) = load(VAR, 0x4e58f0) {
        for call in &calls {
            let mut mem = shared_windows(call);
            let mut host = Clear480Recorded { inner: &call.inner, mem: mem.clone(), next: 0 };
            layout::clear_car_bodies(&mut mem, call.args[0], call.args[1], 0x300000, &mut host);
            host.finish();
            assert_eq!(host.next, call.inner.len(), "Clear480 call {} children", call.index);
            let car = call.args[0];
            let expected =
                &call.windows.iter().find(|(_, at, _, _, _)| *at == car).expect("Clear480 car window").4[0x480..0x490];
            assert_eq!(mem.read(car + 0x480, 0x10), expected, "Clear480 slots call {}", call.index);
        }
    }
    replay(0x51a060, "CEntity::E43_ExclA060", false, |mem, args| {
        layout::remove_exclusion(mem, args[0], args[1]);
        0
    });
}

struct Vf128xRecorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    frame: u32,
    order: Vec<u32>,
}

impl Vf128xRecorded<'_> {
    fn find(&self, addr: u32, args: &[u32]) -> usize {
        self.inner
            .iter()
            .position(|call| call.addr == addr && call.args == args)
            .unwrap_or_else(|| panic!("Vf128x missing nested {addr:#x} {args:x?}"))
    }

    fn apply(&mut self, index: usize) {
        let call = &self.inner[index];
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
    }

    fn finish(&mut self) {
        for index in 0..self.inner.len() {
            self.apply(index);
        }
    }
}

impl rbr_physics::car_body::layout::Vf128xCalls for Vf128xRecorded<'_> {
    fn compose_a(&mut self, sp: u32, this: u32, a: u32, car: u32, out: u32) {
        assert_eq!(sp, self.frame - 0x3c);
        self.order.push(0x51b4c0);
        let index = self.find(0x51b4c0, &[this, a, car, out]);
        self.apply(index);
    }

    fn collide6000(&mut self, sp: u32, car: u32, m: u32, f1: u32, f2: u32) {
        assert_eq!(sp, self.frame - 0x3c);
        self.order.push(0x4e6000);
        let index = self.find(0x4e6000, &[car, m, f1, f2]);
        self.apply(index);
    }

    fn e43vf128(&mut self, sp: u32, car: u32, args: [u32; 8]) {
        assert_eq!(sp, self.frame - 0x50);
        self.order.push(0x52a0e0);
        let mut expected = vec![car];
        expected.extend(args);
        let index = self.find(0x52a0e0, &expected);
        self.apply(index);
    }
}

#[test]
fn car_vf128x_matches_direct_records() {
    let Some(calls) = load(VAR, 0x4e6100) else {
        eprintln!("skipping CCar::Vf128x: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        let compose = call
            .inner
            .iter()
            .find(|i| i.addr == 0x51b4c0 && i.args[1] == call.args[1])
            .expect("Vf128x initial ComposeA record");
        mem.set_u32(0x7cbb00, compose.args[0]);
        let frame = compose.args[3] + 0x1c;
        let args: [u32; 8] = call.args[1..9].try_into().expect("Vf128x eight args");
        let mut host = Vf128xRecorded { inner: &call.inner, mem: mem.clone(), frame, order: Vec::new() };
        rbr_physics::car_body::layout::vf128x(&mut mem, call.args[0], args, frame, &mut host);
        assert_eq!(host.order, [0x51b4c0, 0x4e6000, 0x52a0e0], "Vf128x nested order");
        host.finish();
        check_windows(call, &mem, "CCar::Vf128x");
    }
    eprintln!("CCar::Vf128x: {} direct calls match", calls.len());
}

struct Clear480Recorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    next: usize,
}
impl layout::Clear480Calls for Clear480Recorded<'_> {
    fn sim_fd00(&mut self, _sp: u32, body: u32) -> u32 {
        while self.inner.get(self.next).map(|c| c.addr) != Some(0x51fd00) {
            let nested = self.inner.get(self.next).expect("Clear480 nested SimFD00 record");
            assert!(
                [0x51b4c0, 0x51b560, 0x4e68e0, 0x70bec2].contains(&nested.addr),
                "unexpected Clear480 nested call {:#x}",
                nested.addr
            );
            self.apply(nested);
            self.next += 1;
        }
        let call = self.inner.get(self.next).expect("Clear480 SimFD00 record");
        assert_eq!(call.args, [body]);
        self.apply(call);
        self.next += 1;
        call.eax
    }
}

impl Clear480Recorded<'_> {
    fn apply(&mut self, call: &Inner) {
        for (_, at, len, pre, _) in &call.windows {
            if *at < 0x4000_0000 {
                self.mem.map(*at, pre);
            } else {
                assert_eq!(self.mem.read(*at, *len), *pre, "Clear480 nested pre");
            }
        }
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
    }

    fn finish(&mut self) {
        while let Some(call) = self.inner.get(self.next) {
            assert!(
                [0x51b4c0, 0x51b560, 0x4e68e0, 0x70bec2].contains(&call.addr),
                "unexpected trailing Clear480 call {:#x}",
                call.addr
            );
            self.apply(call);
            self.next += 1;
        }
    }
}

struct F350Recorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    next: usize,
    frame: u32,
    car_target: u32,
}

impl F350Recorded<'_> {
    fn take(&mut self, target: u32, args: &[u32], allowed_skips: &[u32]) -> u32 {
        while let Some(call) = self.inner.get(self.next) {
            if call.addr == target {
                assert_eq!(call.args, args, "SimF350 inner {} args", self.next);
                for (_, at, _, pre, post) in &call.windows {
                    self.mem.map(*at, pre);
                    self.mem.map(*at, post);
                }
                self.next += 1;
                return call.eax;
            }
            assert!(
                allowed_skips.contains(&call.addr),
                "SimF350 inner {} unexpected target {:#x} before {target:#x}",
                self.next,
                call.addr
            );
            for (_, at, _, pre, post) in &call.windows {
                self.mem.map(*at, pre);
                self.mem.map(*at, post);
            }
            self.next += 1;
        }
        panic!("SimF350 is missing inner target {target:#x} with args {args:x?}");
    }

    fn skip_nested(&self) -> &'static [u32] {
        &[0x473cd0, 0x4e68e0, 0x51b4c0, 0x51b560, 0x70bec2]
    }
}

impl layout::F350Calls for F350Recorded<'_> {
    fn xform(&mut self, sp: u32, car: u32, pose: u32, out: u32) {
        assert_eq!(sp, self.frame - 0x7c);
        self.take(0x4e68e0, &[car, pose, out], &[0x51b560]);
    }

    fn construct(&mut self, sp: u32, at: u32) {
        assert_eq!(sp, self.frame - 0x78);
        self.take(0x52f090, &[at, 0], &[]);
    }

    fn scale(&mut self, sp: u32, vector: u32, scalar: f32, out: u32) {
        assert_eq!(sp, self.frame - 0x7c);
        self.take(0x473cd0, &[vector, scalar.to_bits(), out], &[]);
    }

    fn car_impulse(&mut self, sp: u32, car: u32, args: [u32; 8]) {
        assert_eq!(sp, self.frame - 0x94);
        let mut expected = vec![car];
        expected.extend(args);
        self.take(self.car_target, &expected, self.skip_nested());
    }

    fn clear_car(&mut self, sp: u32, car: u32, body: u32) {
        assert_eq!(sp, self.frame - 0x78);
        self.take(0x4e58f0, &[car, body], self.skip_nested());
    }

    fn exclude(&mut self, sp: u32, body: u32, car: u32) {
        assert_eq!(sp, self.frame - 0x78);
        self.take(0x51a060, &[body, car], self.skip_nested());
    }
}

#[test]
fn sim_f350_matches_owner_car_records() {
    let Some(calls) = load(VAR, 0x51f350) else {
        eprintln!("skipping SimF350 direct records: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        let this = call.args[0];
        let car = mem.u32(this + 0x3d0);
        let car_target = layout::vtable_target(&mem, car, 0x128);
        let xform = call.inner.iter().find(|i| i.addr == 0x4e68e0).expect("SimF350 xform record");
        let frame = xform.args[2].wrapping_add(0x50);
        let args: [u32; 8] = call.args[1..9].try_into().expect("SimF350 has eight stack arguments");
        let mut host = F350Recorded { inner: &call.inner, mem: mem.clone(), next: 0, frame, car_target };
        layout::sim_f350(&mut mem, this, args, frame, &mut host);
        assert_eq!(host.next, call.inner.len(), "SimF350 call {}: external calls", call.index);
        check_windows(call, &mem, "CHingedRigidBody::SimF350");
    }
    eprintln!("SimF350: {} owner-car calls match", calls.len());
}

#[test]
fn e90_seg_add_helpers_match_direct_records() {
    replay(0x605F30, "E90Seg::AddParticle", true, |mem, a| layout::add_particle(mem, a[0], a[1], f32::from_bits(a[2])));
    replay(0x605FB0, "E90Seg::AddSpring", true, |mem, a| {
        layout::add_spring(mem, a[0], a[1], a[2], a[3], f32::from_bits(a[4]))
    });
    replay(0x606070, "E90Seg::AddFixed", false, |mem, a| {
        layout::add_fixed(mem, a[0], a[1]);
        0
    });
    replay(0x606090, "E90Seg::AddPin", false, |mem, a| {
        layout::add_pin(mem, a[0], a[1]);
        0
    });
    replay(0x6060B0, "E90Seg::AddBend", false, |mem, a| {
        layout::add_bend(mem, a[0], a[1], a[2]);
        0
    });
}

/// The E90 init group's inner calls, replayed from the records: each must be the next recorded one (target and
/// arguments), its windows must hold the recorded bytes before it, and its recorded bytes after it are applied, as the
/// original's effect. The answer is its eax.
struct E90InitRecorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    next: usize,
}

impl E90InitRecorded<'_> {
    fn take(&mut self, addr: u32, args: &[u32]) -> u32 {
        let call =
            self.inner.get(self.next).unwrap_or_else(|| panic!("E90 init inner {} is missing ({addr:#x})", self.next));
        assert_eq!(call.addr, addr, "E90 init inner {} target", self.next);
        assert_eq!(call.args, args, "E90 init inner {} args", self.next);
        for (_, at, len, pre, _) in &call.windows {
            assert_eq!(self.mem.read(*at, *len), *pre, "E90 init inner {} pre", self.next);
        }
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        self.next += 1;
        call.eax
    }
}

impl layout::E90SegInitCalls for E90InitRecorded<'_> {
    fn normalize(&mut self, _sp: u32, out: u32, v: u32) {
        self.take(0x627235, &[out, v]);
    }
}

impl layout::E90PoolCalls for E90InitRecorded<'_> {
    fn allocate(&mut self, _sp: u32, size: u32) -> u32 {
        self.take(0x70bf84, &[size])
    }
    fn release(&mut self, _sp: u32, block: u32) {
        self.take(0x70c27e, &[block]);
    }
    fn construct(&mut self, _sp: u32, element: u32) {
        self.take(0x602220, &[element]);
    }
}

#[test]
fn e90_seg_init_and_pool_inits_match_direct_records() {
    let Some(calls) = load(VAR, 0x605b60) else {
        eprintln!("skipping E90Seg::Init: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        // The span's temporaries sit at the original's stack: the first normalise's vector is `frame - 0x1c`.
        let frame = call.inner[0].args[0] + 0x1c;
        let mut host = E90InitRecorded { inner: &call.inner, mem: mem.clone(), next: 0 };
        layout::e90_seg_init(&mut mem, call.args[0], call.args[1], frame, &mut host);
        assert_eq!(host.next, call.inner.len(), "E90Seg::Init call {} inner", call.index);
        check_windows(call, &mem, "E90Seg::Init");
    }
    type Init = fn(&mut Mem, u32, u32, u32, u32, &mut E90InitRecorded) -> u32;
    let pools: [(u32, &str, Init); 4] = [
        (0x6026a0, "E90PartPool::Init", |m, p, n, b, f, h| layout::init_e90_part_pool(m, p, n, b, f, h)),
        (0x6027a0, "E90PtrTab::Init", |m, p, n, b, f, h| layout::init_e90_ptr_tab(m, p, n, b, f, h)),
        (0x6028b0, "E90BendPool::Init", |m, p, n, b, f, h| layout::init_e90_bend_pool(m, p, n, b, f, h)),
        (0x602930, "E90PinPool::Init", |m, p, n, b, f, h| layout::init_e90_pin_pool(m, p, n, b, f, h)),
    ];
    for (addr, name, port) in pools {
        let Some(calls) = load(VAR, addr) else {
            eprintln!("skipping {name}: {VAR} not set");
            continue;
        };
        for call in &calls {
            let mut mem = shared_windows(call);
            let mut host = E90InitRecorded { inner: &call.inner, mem: mem.clone(), next: 0 };
            let eax = port(&mut mem, call.args[0], call.args[1], call.args[2], 0x300000, &mut host);
            assert_eq!(eax, call.eax, "{name} call {} eax", call.index);
            assert_eq!(host.next, call.inner.len(), "{name} call {} inner", call.index);
            check_windows(call, &mem, name);
        }
    }
}

/// Answers a sequence port's host calls from a record's inner calls, in order: each must be the next one, at `addr`
/// (when given) and with the same arguments (`this` first). Its windows' pre bytes outside the port's frame must be the
/// port's memory; then its post bytes are mapped.
struct InOrder<'a> {
    name: &'static str,
    index: usize,
    inner: &'a [Inner],
    next: usize,
    frame: u32,
}

impl<'a> InOrder<'a> {
    fn take(&mut self, mem: &mut Mem, addr: Option<u32>, args: &[u32]) -> &'a Inner {
        let (name, index) = (self.name, self.index);
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("{name} call {index}: more calls than recorded"));
        if let Some(addr) = addr {
            assert_eq!(call.addr, addr, "{name} call {index}: inner {}", self.next);
        }
        assert_eq!(call.args, args, "{name} call {index}: inner {} {:#x} args", self.next, call.addr);
        let frame = self.frame.wrapping_sub(0x100)..self.frame.wrapping_add(0x100);
        for (k, at, len, pre, _) in &call.windows {
            if !frame.contains(at) {
                assert_eq!(
                    mem.read(*at, *len),
                    *pre,
                    "{name} call {index}: inner {} {:#x} pre window {k}",
                    self.next,
                    call.addr
                );
            }
        }
        for (_, at, _, _, post) in &call.windows {
            mem.map(*at, post);
        }
        self.next += 1;
        call
    }

    fn finish(&self) {
        assert_eq!(self.next, self.inner.len(), "{} call {}: fewer calls than recorded", self.name, self.index);
    }
}

/// Maps the bytes the inner calls' windows see and the call's own windows do not cover, from their first pre: the
/// state of memory the port does not read, which the inner calls' pre checks compare.
fn seed_inner(mem: &mut Mem, call: &Call) {
    let mut seen: std::collections::HashSet<u32> =
        call.windows.iter().flat_map(|(_, at, len, _, _)| (0..*len as u32).map(move |i| at + i)).collect();
    for inner in &call.inner {
        for (_, at, _, pre, _) in &inner.windows {
            for (i, byte) in pre.iter().enumerate() {
                if seen.insert(at + i as u32) {
                    mem.map(at + i as u32, &[*byte]);
                }
            }
        }
    }
}

impl layout::DA60Calls for InOrder<'_> {
    fn compose_a(&mut self, mem: &mut Mem, _: u32, car: u32, [a, b, c]: [u32; 3]) {
        self.take(mem, Some(0x51b4c0), &[car, a, b, c]);
    }

    fn quat_mul(&mut self, mem: &mut Mem, _: u32, this: u32, b: u32, out: u32) {
        self.take(mem, Some(0x483dd0), &[this, b, out]);
    }
}

impl layout::FD00Calls for InOrder<'_> {
    fn sim_da60(&mut self, mem: &mut Mem, _: u32, body: u32) {
        self.take(mem, Some(0x51da60), &[body]);
    }

    fn to_world(&mut self, mem: &mut Mem, _: u32, car: u32, local: u32, out: u32) {
        self.take(mem, Some(0x4e68e0), &[car, local, out]);
    }

    fn slot(&mut self, mem: &mut Mem, _: u32, body: u32, _slot: u32) {
        self.take(mem, None, &[body]);
    }

    fn clear_accumulators(&mut self, mem: &mut Mem, _: u32, body: u32) {
        self.take(mem, Some(0x529e60), &[body]);
    }

    fn unlink(&mut self, mem: &mut Mem, _: u32, link: u32) {
        self.take(mem, Some(0x4e41a0), &[link]);
    }

    fn init_bd0(&mut self, mem: &mut Mem, _: u32, body: u32) {
        self.take(mem, Some(0x519bd0), &[body]);
    }

    fn lookup_twin(&mut self, mem: &mut Mem, _: u32, grid: u32, x: u32, y: u32) -> u32 {
        self.take(mem, Some(0x4e03b0), &[grid, x, y]).eax
    }

    fn move_to_cell(&mut self, mem: &mut Mem, _: u32, link: u32, cell: u32) {
        self.take(mem, Some(0x4faeb0), &[link, cell]);
    }

    fn reg_wall(&mut self, mem: &mut Mem, _: u32, state: u32, body: u32) {
        self.take(mem, Some(0x5263d0), &[state, body]);
    }

    fn emit_detach(&mut self, mem: &mut Mem, _: u32, signal: u32, sig: u32) {
        let payload = mem.read(sig, 4);
        let call = self.take(mem, Some(0x51f580), &[signal, sig]);
        assert_eq!(payload, call.windows[0].3, "{} call {}: the detach payload", self.name, self.index);
    }

    fn emit_event(&mut self, mem: &mut Mem, _: u32, signal: u32, event: u32) {
        let ours = mem.read(event, 0x50);
        let call = self.take(mem, Some(0x51f7c0), &[signal, event]);
        let theirs = &call.windows[0].3;
        // The words at +0x8 and +0xc are the original's stack, not the event's.
        assert_eq!(ours[..8], theirs[..8], "{} call {}: the event's head", self.name, self.index);
        assert_eq!(ours[0x10..], theirs[0x10..], "{} call {}: the event's poses", self.name, self.index);
    }
}

impl layout::StepBigCalls for InOrder<'_> {
    fn world_compose(&mut self, mem: &mut Mem, _: u32, car: u32, point: u32, out: u32) {
        self.take(mem, Some(0x4e6950), &[car, point, out]);
    }

    fn quat_norm(&mut self, mem: &mut Mem, _: u32, quat: u32, out: u32) {
        self.take(mem, Some(0x4e4c70), &[quat, out]);
    }
}

/// `CMountedRigidBody::StepBig` (0x522100): direct records on the mounted bodies of the V2 drives' detach snapshots
/// (`harness/vector_specs/g_stepbig.py`), every call checked in order.
#[test]
fn step_big_matches_direct_records() {
    let Some(calls) = load(VAR, 0x522100) else {
        eprintln!("skipping StepBig: {VAR} not set");
        return;
    };
    let mut detached = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        seed_inner(&mut mem, call);
        // The event is the last call's argument, 0x60 above the frame.
        let frame = call.inner.last().map_or(0, |emit| emit.args[1] - 0x60);
        let mut host = InOrder { name: "StepBig", index: call.index, inner: &call.inner, next: 0, frame };
        let eax = layout::step_big(&mut mem, call.args[0], frame, &mut host);
        host.finish();
        assert_eq!(eax, call.eax, "StepBig call {}: eax", call.index);
        check_windows(call, &mem, "CMountedRigidBody::StepBig");
        detached += eax as usize;
    }
    assert!(detached > 0, "StepBig: no record detaches");
    eprintln!("StepBig: {} direct calls match, {detached} detached", calls.len());
}

#[test]
fn sim_fd00_matches_direct_records() {
    let Some(calls) = load(VAR, 0x51fd00) else {
        eprintln!("skipping SimFD00: {VAR} not set");
        return;
    };
    let mut detached = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        seed_inner(&mut mem, call);
        // The event is the last call's argument, 0x40 above the frame.
        let frame = call.inner.last().map_or(0, |emit| emit.args[1] - 0x40);
        let mut host = InOrder { name: "SimFD00", index: call.index, inner: &call.inner, next: 0, frame };
        let eax = layout::sim_fd00(&mut mem, call.args[0], frame, &mut host);
        host.finish();
        assert_eq!(eax, call.eax, "SimFD00 call {}: eax", call.index);
        check_windows(call, &mem, "CHingedRigidBody::SimFD00");
        detached += eax as usize;
    }
    eprintln!("SimFD00: {} direct calls match, {detached} detached", calls.len());
}

#[test]
fn sim_da60_matches_direct_records() {
    let Some(calls) = load(VAR, 0x51da60) else {
        eprintln!("skipping SimDA60: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        seed_inner(&mut mem, call);
        let frame = call.inner[0].args[3] - 0x20;
        let mut host = InOrder { name: "SimDA60", index: call.index, inner: &call.inner, next: 0, frame };
        layout::sim_da60(&mut mem, call.args[0], frame, &mut host);
        host.finish();
        check_windows(call, &mem, "CHingedRigidBody::SimDA60");
    }
    eprintln!("SimDA60: {} direct calls match", calls.len());
}

#[test]
fn detach_leaves_match_direct_records() {
    use rbr_physics::physics_core::layout as core;
    replay(0x529e60, "CSolid::E43ClearAcc", true, |mem, a| layout::clear_accumulators(mem, a[0]));
    replay(0x519bd0, "CEntity::E43_InitBD0", true, |mem, a| layout::init_bd0(mem, a[0]));
    replay(0x4e6af0, "E34LinkList10::PushItem", true, |mem, a| {
        core::push_item(mem, a[0], a[1]);
        a[1]
    });
    replay(0x4faeb0, "CBodyLink::MoveToCell", true, |mem, a| {
        rbr_physics::collision::layout::move_to_cell(mem, a[0], a[1]);
        a[1]
    });
    if let Some(calls) = load(VAR, 0x4e41a0) {
        for call in &calls {
            let mut mem = map_windows(call);
            let listed = mem.u32(call.args[0] + 8) != 0;
            let eax = core::unlink_a(&mut mem, call.args[0], call.eax);
            if listed {
                assert_eq!(eax, call.eax, "UnlinkA call {}: eax", call.index);
            }
            check_windows(call, &mem, "E34Link10::UnlinkA");
        }
        eprintln!("UnlinkA: {} direct calls match", calls.len());
    }
    if let Some(calls) = load(VAR, 0x5263d0) {
        for call in &calls {
            let mut mem = map_windows(call);
            let object = call.args[1];
            let mut slots = call.inner.iter();
            let eax = core::reg_wall(&mut mem, call.args[0], object, |_, _| {
                let asked = slots.next().expect("RegWall slot record");
                assert_eq!(asked.args, [object], "RegWall call {}: slot", call.index);
                asked.eax != 0
            });
            // The third list and none go through `PushItem`, which the port makes inline.
            if let Some(push) = slots.next() {
                assert_eq!(push.addr, 0x4e6af0, "RegWall call {}: fewer slots", call.index);
                assert_eq!(
                    push.args,
                    [core::entity_list(call.args[0], false, false, push.args[0] == call.args[0] + 0x470), object]
                );
            }
            assert!(slots.next().is_none(), "RegWall call {}: more calls", call.index);
            assert_eq!(eax, call.eax, "RegWall call {}: eax", call.index);
            check_windows(call, &mem, "CPhysics::CState::RegWall");
        }
        eprintln!("RegWall: {} direct calls match", calls.len());
    }
}

/// `E37GridOps::Copy6770` (`g-ent-proofs` direct records): the 0x6b0-byte grid copied, nothing for a null `this`, eax
/// `this` (the binding's).
#[test]
fn copy_6770_matches_direct_records() {
    let Some(calls) = load(VAR, 0x4F6770) else {
        eprintln!("skipping E37GridOps::Copy6770: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "E37GridOps::Copy6770: no calls in the vector files");
    for call in &calls {
        let mut mem = map_windows(call);
        layout::copy_grid(&mut mem, call.args[0], call.args[1]);
        assert_eq!(call.eax, call.args[0], "Copy6770 call {}: eax", call.index);
        check_windows(call, &mem, "E37GridOps::Copy6770");
    }
    eprintln!("E37GridOps::Copy6770: {} calls match", calls.len());
}
