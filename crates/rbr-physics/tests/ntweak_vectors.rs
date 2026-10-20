//! Replays the original's `CTokenCursor::Tokenize` and `E11_StackAlloc` calls (`harness/direct_calls.py --set
//! g-ntweak`, in the physics core's store; skipped when the store has none) through `rbr_physics::ntweak`: the cursor, the table from `operator new`, the
//! text cut in place, the heap record and the block.

mod common;

use common::{check_windows, load, map_windows};
use rbr_physics::ntweak;

const VAR: &str = "RBR_PHYSICS_CORE_VECTORS";
const TOKENIZE: u32 = 0x52ec80;
const STACK_ALLOC: u32 = 0x473a70;
const OPERATOR_NEW: u32 = 0x70bec2;

#[test]
fn tokenize_matches() {
    let Some(calls) = load(VAR, TOKENIZE).filter(|c| !c.is_empty()) else {
        eprintln!("skipping Tokenize: no calls ({VAR})");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let mut news = Vec::new();
        let alloc = |mem: &mut rbr_physics::mem::Mem, size: u32| {
            news.push(size);
            let block = call.inner[0].eax;
            mem.map(block, &vec![0; size as usize]);
            block
        };
        ntweak::tokenize(&mut mem, alloc, call.args[0], call.args[1])
            .unwrap_or_else(|e| panic!("Tokenize call {}: {e:?}", call.index));
        let expected: Vec<_> = call.inner.iter().map(|i| (i.addr, i.args.clone())).collect();
        let made: Vec<_> = news.iter().map(|&size| (OPERATOR_NEW, vec![size])).collect();
        assert_eq!(made, expected, "Tokenize call {}: calls", call.index);
        assert_eq!(call.eax, call.args[0], "Tokenize call {}: eax", call.index);
        check_windows(call, &mem, "Tokenize");
    }
    eprintln!("Tokenize: {} calls match", calls.len());
}

#[test]
fn stack_alloc_matches() {
    let Some(calls) = load(VAR, STACK_ALLOC).filter(|c| !c.is_empty()) else {
        eprintln!("skipping E11_StackAlloc: no calls ({VAR})");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let [size, heap, align] = call.args[..3] else { unreachable!() };
        let block = ntweak::stack_alloc(&mut mem, size, heap, align)
            .unwrap_or_else(|e| panic!("E11_StackAlloc call {}: {e:?}", call.index));
        assert_eq!(block, call.eax, "E11_StackAlloc call {}: block", call.index);
        check_windows(call, &mem, "E11_StackAlloc");
    }
    eprintln!("E11_StackAlloc: {} calls match", calls.len());
}

/// The calls `Parse` makes outside the walk, as the test sees them.
#[derive(Default)]
struct Calls {
    next_table: u32,
    calls: Vec<String>,
}

impl ntweak::ParseHost for Calls {
    fn alloc(&mut self, mem: &mut rbr_physics::mem::Mem, size: u32) -> u32 {
        self.next_table += 0x10000;
        mem.map(self.next_table, &vec![0; size as usize]);
        self.calls.push(format!("new {size:#x} -> {:#x}", self.next_table));
        self.next_table
    }

    fn free(&mut self, _: &mut rbr_physics::mem::Mem, at: u32) {
        self.calls.push(format!("delete {at:#x}"));
    }

    fn unk14(&mut self, _: &mut rbr_physics::mem::Mem, _: u32, _: u32, _: u32, _: u32) {
        unreachable!("the object's slot 5 is CBase's")
    }

    fn member_parse(&mut self, _: &mut rbr_physics::mem::Mem, _: u32, _: u32) {
        unreachable!("the spring-damper definition has no member objects")
    }

    fn after_parse(&mut self, _: &mut rbr_physics::mem::Mem, this: u32) {
        self.calls.push(format!("after {this:#x}"));
    }

    fn def_index(&mut self, _: &mut rbr_physics::mem::Mem, _: &str) -> Option<u32> {
        unreachable!("not the definition manager")
    }
}

/// `CBase::Parse` on a spring-damper definition: the group cut into a table, its name and an inner group's items on
/// the token heap, the values stored, slot 17, both tables freed; the caller's cursor moves past the group.
#[test]
fn parse_cuts_each_group_and_walks_its_tokens() {
    use rbr_formats::lsp_keys::Class;
    use rbr_physics::mem::Mem;
    use rbr_physics::Variant;

    const THIS: u32 = 0x1000;
    const VTABLE: u32 = 0x2000;
    const REAL: u32 = 0x2100;
    const UNSIGNED: u32 = 0x2200;
    const HEAP: u32 = 0x3000;
    const NAMES: u32 = 0x4000;
    const TEXT: u32 = 0x5000;
    const CURSOR: u32 = 0x6000;
    const FRAME: ntweak::Frame = ntweak::Frame { tokens: 0x7000, inner: 0x7008 };
    let mut mem = Mem::new();
    mem.map(THIS, &[0; 0x120]);
    mem.map(VTABLE, &[0; 0x48]);
    mem.set_u32(THIS, VTABLE);
    mem.set_u32(VTABLE + 0x14, 0x52fe80);
    mem.map(REAL, &[0; 0x10]);
    mem.set_u32(REAL + 8, 0x52f770);
    mem.map(UNSIGNED, &[0; 0x10]);
    mem.set_u32(UNSIGNED + 8, 0x52f840);
    mem.set_u32(THIS + 0x0c, UNSIGNED);
    for at in (0x1c..0x11c).step_by(0x10) {
        mem.set_u32(THIS + at, REAL);
    }
    mem.map(ntweak::TOKEN_HEAP, &[0; 4]);
    mem.map(HEAP, &[0; 12]);
    mem.set_u32(ntweak::TOKEN_HEAP, HEAP);
    mem.set_u32(HEAP, NAMES);
    mem.set_u32(HEAP + 8, 0x1000);
    mem.map(NAMES, &[0xcc; 0x1000]);
    // A group as the cursor's token has it: its closing parenthesis already cut.
    let text = b"(\"SpringDamper\" InnerBumpRubberPos 0.17 (\"G\" (\"a\") (\"bb\")) DebugID +2 12 SpringLength\0";
    mem.map(TEXT, text);
    mem.map(CURSOR, &[0; 16]);
    mem.set_u32(CURSOR, CURSOR + 8);
    mem.set_u32(CURSOR + 4, CURSOR + 8);
    mem.set_u32(CURSOR + 8, TEXT);
    mem.map(FRAME.tokens, &[0; 16]);
    let mut host = Calls::default();
    ntweak::parse(&mut mem, &mut host, Variant::Stock102, FRAME, THIS, Class::SpringDef, CURSOR).unwrap();
    assert_eq!(
        host.calls,
        ["new 0x4000 -> 0x10000", "new 0x4000 -> 0x20000", "delete 0x20000", "after 0x1000", "delete 0x10000"]
    );
    assert_eq!(mem.read(NAMES, 13), b"SpringDamper\0");
    assert_eq!(mem.u32(THIS + 4), NAMES);
    assert_eq!(mem.read(NAMES + 0x10, 2), b"a\0");
    assert_eq!(mem.read(NAMES + 0x30, 3), b"bb\0");
    assert_eq!(mem.u32(HEAP + 4), 0x50);
    assert_eq!(mem.f32(THIS + 0x1c + 0xc), 0.17);
    assert_eq!(mem.u32(THIS + 0x0c + 0xc), 2);
    assert_eq!(mem.u32(CURSOR + 4), CURSOR + 12);
    // The tables: the group's eight tokens, the inner group's three; the text cut at each token's end.
    let words: Vec<u32> = (0..9).map(|k| mem.u32(0x10000 + 4 * k)).collect();
    assert_eq!(words, [TEXT + 1, TEXT + 16, TEXT + 35, TEXT + 40, TEXT + 59, TEXT + 67, TEXT + 70, TEXT + 73, 0]);
    let inner: Vec<u32> = (0..4).map(|k| mem.u32(0x20000 + 4 * k)).collect();
    assert_eq!(inner, [TEXT + 41, TEXT + 45, TEXT + 51, 0]);
    let after = mem.read(TEXT, text.len());
    let cut: Vec<usize> = (0..text.len()).filter(|&k| after[k] != text[k]).collect();
    assert_eq!(cut, [14, 34, 39, 43, 49, 56, 57, 66, 69, 72]);
    assert!(cut.iter().all(|&k| after[k] == 0));
}
