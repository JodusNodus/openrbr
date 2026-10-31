use super::stand_in::{code_stand_in, dll_listener, CODE_STAND_INS, DLL_LISTENERS};
use super::*;
use rbr_physics::mem::Backend;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

#[test]
fn allocator_views_keep_the_boot_runtime_without_mapping_code() {
    use rbr_physics::ngp::layout::runtime::Runtime;
    let mut mem = Mem::with_pages(rbr_physics::mem::Pages::default());
    mem.set_ngp_runtime(Runtime::new([0x1000, 0x2000, 0x3000], 10));
    let runtime = mem.ngp_runtime().unwrap().clone();
    let mut pure = Pure::new(mem, false);
    assert!(Arc::ptr_eq(pure.mem.ngp_runtime().unwrap(), &runtime));
    pure.untrack_allocator();
    assert!(Arc::ptr_eq(pure.mem.ngp_runtime().unwrap(), &runtime));
    assert!(pure.mem.try_read(0x50d9c4, 1).is_none());
    assert!(pure.mem.try_read(0x4f13a7, 4).is_none());
}

#[test]
fn leaves_are_bound() {
    for addr in LEAVES {
        assert!(binding_slot(addr).is_some(), "{addr:#x}");
    }
}

#[test]
fn code_stand_ins_match_the_exe() {
    let Some(data) = rbr_formats::data::Data::from_env().filter(|d| d.patch().is_some()) else {
        eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
        return;
    };
    let path = data.resolve("RichardBurnsRally_SSE.exe").expect("the exe");
    let exe = rbr_formats::pe::parse(&std::fs::read(path).unwrap()).unwrap();
    for (at, stand_in) in CODE_STAND_INS {
        let code = exe.read(at, 19).expect("code in the exe");
        assert_eq!(code_stand_in(code), Some(stand_in), "{at:#x}");
    }
}

/// Memory shared by its clones, as a snapshot's is.
#[derive(Default)]
struct Shared(Mutex<BTreeMap<u32, u8>>);

impl Backend for Shared {
    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        let bytes = self.0.lock().unwrap();
        for (i, b) in out.iter_mut().enumerate() {
            match bytes.get(&(at + i as u32)) {
                Some(&v) => *b = v,
                None => return false,
            }
        }
        true
    }

    fn write(&self, at: u32, src: &[u8]) {
        let mut bytes = self.0.lock().unwrap();
        for (i, &b) in src.iter().enumerate() {
            bytes.insert(at + i as u32, b);
        }
    }
}

const TRAMPOLINE: u32 = 0x401000;
const UNBOUND: u32 = 0x402000;
const RET_4: u32 = 0x403000;
const CONN: u32 = 0x20000;
const OWNER: u32 = 0x30000;

/// A stack at 0x10000-0x12000, the import slot, a connection, and code: an `Invoke` trampoline, a function with
/// no binding and one that is `ret 4`.
fn host(keep_going: bool) -> Pure {
    let mut mem = Mem::with_backend(Arc::new(Shared::default()));
    mem.map(0x10000, &[0; 0x2000]);
    mem.map(QUERY_PERFORMANCE_COUNTER, &[0; 4]);
    for at in [UNBOUND, CRT_SIN] {
        mem.map(at, &[0x90; 64]);
    }
    let code = |bytes: &[u8]| [bytes, &[0xcc; 64][bytes.len()..]].concat();
    mem.map(TRAMPOLINE, &code(&[0x8b, 0xc1, 0x8b, 0x48, 0x10, 0xff, 0x60, 0x14]));
    mem.map(RET_4, &code(&[0xc2, 0x04, 0x00]));
    mem.map(CONN, &[0; 0x18]);
    mem.set_u32(CONN + 0x10, OWNER);
    Pure::new(mem, keep_going)
}

/// Calls `addr` from a frame of `StepForward` with its entry at 0x11ff0.
fn call(pure: &Pure, addr: u32, ecx: u32, args: &[u32]) -> Called {
    let regs = Regs { eax: 0x77, esp: 0x11ff0, ..Regs::default() };
    pure.with_frame(STEP_FORWARD, regs, || pure.call(addr, ecx, args))
}

fn stop_of(pure: &Pure, addr: u32, ecx: u32) -> Stop {
    let e = panic::catch_unwind(AssertUnwindSafe(|| call(pure, addr, ecx, &[1]))).unwrap_err();
    e.downcast_ref::<Stopped>().expect("a stop").0.clone()
}

#[test]
fn a_call_with_no_binding_stops_with_its_address_and_caller() {
    let pure = host(false);
    let unbound = Stop::Unbound { addr: UNBOUND, caller: STEP_FORWARD };
    assert_eq!(stop_of(&pure, UNBOUND, 0), unbound);
    // Through a trampoline, the stop is the method it leads to.
    pure.mem.clone().set_u32(CONN + 0x14, UNBOUND);
    assert_eq!(stop_of(&pure, TRAMPOLINE, CONN), unbound);
    assert_eq!(uncovered(&pure.mem, TRAMPOLINE, CONN, &[1]), Some(UNBOUND));
}

#[test]
fn keep_going_counts_the_stop_and_answers_zero() {
    let pure = host(true);
    assert_eq!(call(&pure, UNBOUND, 0, &[]).eax, 0);
    assert_eq!(call(&pure, UNBOUND, 0, &[]).eax, 0);
    let unbound = Stop::Unbound { addr: UNBOUND, caller: STEP_FORWARD };
    assert_eq!(pure.stops.take(), BTreeMap::from([(unbound, 2)]));
}

#[test]
fn a_call_runs_below_its_caller_with_the_return_slot_and_arguments() {
    let pure = host(false);
    let mut mem = pure.mem.clone();
    mem.set_u32(CONN + 0x14, RET_4);
    // The trampoline leaves eax = the connection; `ret 4` keeps it.
    assert_eq!(call(&pure, TRAMPOLINE, CONN, &[0x1234]).eax, CONN);
    let sp = (0x11ff0 - 0x1000) & !0xf;
    assert_eq!((mem.u32(sp), mem.u32(sp + 4)), (CALL_RET, 0x1234));
    assert_eq!(uncovered(&pure.mem, TRAMPOLINE, CONN, &[0x1234]), None);
}

#[test]
fn a_dll_listener_is_known_at_any_load_address() {
    let (bytes, Some(at), _) = DLL_LISTENERS[1] else { panic!("the CSigCarControl listener relocates") };
    for entry in [0x6e70ac0u32, 0x7470ac0] {
        let mut code = bytes.to_vec();
        let word = u32::from_le_bytes(code[at..at + 4].try_into().unwrap());
        code[at..at + 4].copy_from_slice(&word.wrapping_add(entry).to_le_bytes());
        assert_eq!(dll_listener(&code, entry), Some(4));
        assert_eq!(dll_listener(&code, entry + 0x10000), None);
    }
}

#[test]
fn the_crt_sine_is_the_x87_model_and_keeps_eax() {
    let pure = host(false);
    let angle = 0.7f32;
    let r = call(&pure, CRT_SIN, 0, &[angle.to_bits()]);
    assert_eq!((r.eax, r.st0.to_bits()), (0x77, trig::x87_sin(angle).to_bits()));
}

#[test]
fn the_pure_crt_allocator_aligns_reuses_by_size_and_zeroes_new_blocks() {
    let mut mem = Mem::new();
    mem.map(0x10000, &[0; 0x2000]);
    for at in [CRT_NEW, CRT_NEW_ARRAY, CRT_DELETE, CRT_DELETE_ARRAY] {
        mem.map(at, &[0x90; 64]);
    }
    let pure = Pure::new(mem, false);
    let a = call(&pure, CRT_NEW_ARRAY, 0, &[0x21]).eax;
    let b = call(&pure, CRT_NEW, 0, &[0x30]).eax;
    assert_eq!(a % 16, 0);
    assert_eq!(b % 16, 0);
    assert_ne!(a, b);
    pure.mem.clone().map(a, &[0xaa; 0x21]);
    call(&pure, CRT_DELETE_ARRAY, 0, &[a]);
    call(&pure, CRT_DELETE, 0, &[b]);
    assert_eq!(call(&pure, CRT_NEW, 0, &[0x30]).eax, b);
    assert_eq!(call(&pure, CRT_NEW_ARRAY, 0, &[0x21]).eax, a);
    assert_eq!(pure.mem.read(a, 0x21), vec![0; 0x21]);

    {
        let _run = PureRunGuard::new(&pure.run_depth);
        assert_eq!(pure.mem.read(a, 4), [0, 0, 0, 0]);
    }
    assert_eq!(pure.allocator_unwritten_reads(), (a..a + 4).collect::<Vec<_>>());
}
