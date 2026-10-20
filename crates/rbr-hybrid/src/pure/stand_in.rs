//! The code outside the bindings that the pure host runs as its known effect ([`StandIn`]): the listeners it skips,
//! the connections' `Invoke` trampolines, the CRT stand-ins, by address in the exe and by their code in the DLLs.

use rbr_physics::mem::Mem;

use super::{
    CRT_COS, CRT_DELETE, CRT_DELETE_ARRAY, CRT_NEW, CRT_NEW_ARRAY, CRT_SIN, QUERY_PERFORMANCE_COUNTER, VECTOR_CTOR,
    VECTOR_DTOR,
};
use crate::binding;

/// The listeners the step reaches outside the physics, `(entry, bytes its ret pops)`, which a pure step skips (eax
/// unchanged): the car and the wheels don't depend on them. rbr-recomp's pure_check skips all of them in a hybrid chain
/// (30 steps from each of the 9 vcu-cov snapshots and snapH.snap-1295, drive-tanner-corolla.bin): the car and wheel
/// words equal the unskipped chain's after every step; only the listeners' own state differs. The brake-wall, damping
/// and body-event listeners (0x476f10, 0x5dd460, 0x61fcf0) only the impact drives reach: the same check on
/// vcu-tanner-damage 6650 and 11827 and vcu-arctic-snow 7000 and 12071 (100 steps) finds no car or wheel word that
/// differs from the hybrid's. The menu and replay receivers of the standstill and gear-change signals (0x5735b0 and
/// 0x61f4d0) are checked the same way: 300 steps on the 10 corolla snapshots, tanner-damage 1200 and 3140, arctic-snow
/// 1500 and 4000, and tarmac-lancer 20500 and 9591 find no car or wheel word that differs from the hybrid's. The damage
/// emit's receivers that write only force feedback, sound, particles, the replay or the ghost (0x491b10 the FFB
/// handler, 0x61f7a0 the replay's damage snapshot, 0x5e3e50 the debris, 0x4b3120 the collision sound, and CSigRespawn's
/// ghost flag at 0x493c80) are checked the same way: 100 steps on the impact snapshots tanner-damage 3140, 6650 and
/// 11827, arctic-snow 12071, v1 gravel 2326 and the nine v2 impacts (the respawn one reached on v2 gravel-corolla-21
/// and tarmac-impreza00-45 only) find no car or wheel word that differs from the hybrid's. The engine-fire and speech
/// receivers (0x5e3f10, 0x4afa70, 0x4d52b0) are not listed: no drive reaches their emits (0x4ef290, 0x4ae0d0). The
/// HedgeHog3D receivers of CSigCarDamage are the connections' 8-byte `Invoke` trampolines (which 30 places in
/// HedgeHog3D.dll share), so a pure step follows them to CHH_World::OnCarDamage and CHH_Car::OnCarDamage, which
/// [`DLL_LISTENERS`] knows by their bytes: 100 steps on tanner-damage 3140, 6650 and 11827 and arctic-snow 12071 (16,
/// 44, 17 and 5 calls skipped) find no car or wheel word that differs from the hybrid's. The detach signal's receivers,
/// the one in LocoMotion.dll (0x1cdb0, `CLM_CharacterCar::OnDetach`, which sets a node's +0x100 flag, the animation's
/// parent matrix) and the one in HedgeHog3D.dll (0x16ed0, `CHH_Car::OnDetachFromParent`, which finds the node of the
/// animation object and calls `SetUnkD2Recursive` on it: graphics), are in [`DLL_LISTENERS`] too: 100 steps on
/// tanner-damage 11827 (one call each skipped) find no car or wheel word that differs from the hybrid's. The other
/// impacts do not reach the LocoMotion one in 100 steps. The DLLs' listeners are in [`DLL_LISTENERS`].
///
/// The outside-assistance and retire receivers are listed too: `E62GameMenu::M573900` (0x573900, the HUD's
/// OutsideAssistance handler, stored at HUD+0x7c8 on the emitter's +0x208 list, 0xd580: `mov [ecx+0x278], edx; movss
/// [ecx+0x270], 0; ret 4`; +0x270 is read only by 0x573920, which the menu-item predicate 0x56a620 calls),
/// `E60Flow::M5662c0` (0x5662c0, the race flow's RetireCar handler, in its connection at +0xd14 on the emitter's +0x208
/// list, 0xadb8; `ret 4` on every path; the payload is always 3, `E34_RetireA` stores it before the emit: `mov [esp+8], 3`),
/// which sets the replay bank's filter flag (`cmp [ecx], 7`, `CReplayManager::Set48_1`) or, for payload 3, calls
/// `HUD::EmitRallyIncident` (HUD+0x27c; the rally-incident bus, whose receivers are the HUD and the sound), and
/// `CMrReplayCar::E93OnAssist` (0x6157e0, the replay's assist record: `E92PushLoad2` into the replay's vector at
/// +0x234, only when the replay flag at `[this+0x80]+0x10` is set). The standalone runs past each of them on the two
/// drives (the next stop is downstream of the emit, see the report). The three are replay, sound or menu state.
///
/// The time keeper the car calls directly is not among them: `CTimeKeeper::Update` (0x4a0320) returns the count
/// `CCar::Update9AF0` takes (nonzero, the car goes on to the timer gap 0x4a0290 and the stand-still tests), and
/// `Record` (0x4a0540) writes the card `Update` reads. Both are bound in [`crate::timekeeper`].
const LISTENERS: [(u32, u32); 24] = [
    (0x476f30, 4), // VCAnimationSpline, CSigCarControl
    (0x476f10, 4), // VCAnimationSpline (the school's checkpoint), CSigCarBrakeWallPenetration
    (0x4b06a0, 4), // CCarSound::M6a0, CSigEngineNoise
    (0x5dd400, 4), // particles (E82), CSigEngineNoise
    (0x5dd460, 4), // particles (E82, E82E_Sys::ScanEmit), CSolid::Emit's damping event
    (0x5fe250, 4), // CAnimatedReplayManager (E89), CSigEngineNoise
    (0x61fc10, 4), // CMrReplayManager (E94), CSigMrRBodyState
    (0x61fcf0, 4), // CMrReplayManager (E94), CSigMrRBodyEvent
    (0x4996b0, 4), // CReplayManager::PosFwd, CSigCarPosition
    (0x4d6560, 4), // E31Tire::M6560 (sound), CSigTireParticles
    (0x5e3050, 4), // E83, CSigTireParticles
    (0x5e4a90, 4), // E84, CSigTireParticles
    (0x5f7040, 4), // E87, CSigTireParticles
    (0x5fe2b0, 4), // CAnimatedReplayManager (E89), CSigTireParticles
    (0x5735b0, 4), // E62GameMenu::M5735b0 (the menu's mode map), CSigCarStandStill
    (0x61f4d0, 4), // E94Manager::E94DispatchB (the replay), CSigMrRCarGearChange
    (0x491b10, 4), // E17_ConnFFBDamage::Vf04 (force feedback), CSigCarDamage
    (0x61f7a0, 4), // E94Conn::E94ConnFwdE (the replay's damage snapshot), CSigCarDamage
    (0x5e3e50, 4), // E83 particles (EmitDebris), CSigCarDamage
    (0x4b3120, 4), // E24Conn_CarDamage::Invoke (CCollisionSnd::OnDamage, sound), CSigCarDamage
    (0x493c80, 4), // E18_ConnGhostRespawn::Vf04 (CGhostCarManager::SetPlayback, the ghost), CSigRespawn
    (0x573900, 4), // E62GameMenu::M573900 (the HUD's handler, the menu's timer), CSigOutsideAssistance
    (0x6157e0, 4), // CMrReplayCar::E93OnAssist (the replay's assist record), CSigOutsideAssistance
    (0x5662c0, 4), // E60Flow::M5662c0 (the race flow's RetireCar handler: replay flag, rally incident), CSigRetireCar
];

/// The listeners in the DLLs, which load at other addresses from run to run (LocoMotion.dll at 0x6e50000 in the
/// tanner-corolla and tarmac-lancer snapshots, 0x7250000 in tanner-damage's, 0x7450000 in arctic-snow's), known by
/// their first bytes: `(bytes, at, pop)`, with the absolute address the loader relocates at `bytes[at..at + 4]` given
/// relative to the entry (`None`: the code has no absolute address, its calls are relative). HedgeHog3D.dll is the
/// patch-1.02 build, the one the game loads (its load address: 0x048f0000 in the damage drives, 0x0af00000 in the
/// composed start).
pub(super) const DLL_LISTENERS: [(&[u8], Option<usize>, u32); 6] = [
    // HedgeHog3D.dll +0x1420, CSigTireParticles: mov eax, [esp + 4]; movss xmm0, [eax + 0x88]; comiss xmm0, [abs]
    (
        &[
            0x8b, 0x44, 0x24, 0x04, 0xf3, 0x0f, 0x10, 0x80, 0x88, 0x00, 0x00, 0x00, 0x0f, 0x2f, 0x05, 0xa8, 0x2d, 0x1b,
            0x00,
        ],
        Some(15),
        4,
    ),
    // LocoMotion.dll +0x20ac0, CSigCarControl: mov eax, [abs]; push esi; mov esi, ecx; mov ecx, [eax + 0x50];
    // mov edx, [ecx]; call [edx + 0x10]
    (&[0xa1, 0xe4, 0xb9, 0x04, 0x00, 0x56, 0x8b, 0xf1, 0x8b, 0x48, 0x50, 0x8b, 0x11, 0xff, 0x52, 0x10], Some(1), 4),
    // HedgeHog3D.dll +0x66e90 (patch-1.02), CHH_World::OnCarDamage, CSigCarDamage: mov eax, [esp + 4]; push esi;
    // mov esi, [eax + 0x40]; ...; mov edi, g_carDamage (the copy's destination); ret 4
    (
        &[
            0x8b, 0x44, 0x24, 0x04, 0x56, 0x8b, 0x70, 0x40, 0x85, 0xf6, 0x74, 0x1b, 0x8b, 0x48, 0x44, 0xc1, 0xe1, 0x02,
            0x8b, 0xc1, 0x57, 0xc1, 0xe9, 0x02, 0xbf, 0xf0, 0x66, 0x17, 0x00,
        ],
        Some(25),
        4,
    ),
    // HedgeHog3D.dll +0x16d50 (patch-1.02), CHH_Car::OnCarDamage, CSigCarDamage: push edi; mov edi, ecx;
    // mov ecx, [esp + 8]; ...; mov eax, [g_HH_DmgFloatA]; ret 4
    (
        &[
            0x57, 0x8b, 0xf9, 0x8b, 0x4c, 0x24, 0x08, 0x8b, 0x01, 0x83, 0xf8, 0x03, 0x75, 0x0f, 0x8b, 0x41, 0x08, 0x50,
            0x6a, 0x00, 0xe8, 0x97, 0x01, 0x00, 0x00, 0x5f, 0xc2, 0x04, 0x00, 0x83, 0xf8, 0x04, 0x0f, 0x85, 0x42, 0x01,
            0x00, 0x00, 0x8b, 0x41, 0x08, 0x83, 0xf8, 0x04, 0x0f, 0x85, 0x3a, 0x01, 0x00, 0x00, 0xa1, 0xac, 0x73, 0x1c,
            0x00,
        ],
        Some(51),
        4,
    ),
    // LocoMotion.dll +0x1cdb0 (patch-1.02), CLM_CharacterCar::OnDetach, CSigDetachFromParent: mov eax, [esp + 4];
    // mov eax, [eax]; mov ecx, [ecx + 0x2c]; push eax; push ecx; call LM_FindNodeByName (relative); ...; mov byte
    // [eax + 0x100], 1; ret 4
    (
        &[
            0x8b, 0x44, 0x24, 0x04, 0x8b, 0x00, 0x8b, 0x49, 0x2c, 0x50, 0x51, 0xe8, 0xf0, 0xfc, 0xff, 0xff, 0x83, 0xc4,
            0x08, 0x85, 0xc0, 0x74, 0x07, 0xc6, 0x80, 0x00, 0x01, 0x00, 0x00, 0x01, 0xc2, 0x04, 0x00,
        ],
        None,
        4,
    ),
    // HedgeHog3D.dll +0x16ed0 (patch-1.02), CHH_Car::OnDetachFromParent, CSigDetachFromParent: mov eax, [esp + 4];
    // mov edx, [eax]; mov ecx, [ecx + 0x13c]; push edx; call FindByAnimObject (relative); ...; ret 4
    (
        &[
            0x8b, 0x44, 0x24, 0x04, 0x8b, 0x10, 0x8b, 0x89, 0x3c, 0x01, 0x00, 0x00, 0x52, 0xe8, 0x5e, 0xd5, 0x01, 0x00,
            0x85, 0xc0, 0x74, 0x0f, 0xc7, 0x44, 0x24, 0x04, 0x01, 0x00, 0x00, 0x00, 0x8b, 0xc8, 0xe9, 0x1b, 0xd4, 0x01,
            0x00, 0xc2, 0x04, 0x00,
        ],
        None,
        4,
    ),
];

/// The code stand-ins the step and the boot's `CCar::UpdateA720` reach (a `ret n`, an `Invoke` trampoline), by address,
/// for an image without the exe's `.text` (`rbr-standalone --boot`): those the composed snapshot's whole-stage run met,
/// the drive's hook under `UpdateMrr`, and 0x519e10, which the boot's run reaches
/// after it differs. 0x519dd0 is `CEntity`'s empty `Vf128x` slot, which the solver's `Emit4DC0` calls on a body (the
/// boot's run reaches it too), and 0x519bc0 its empty slot 9 (`E43Vf24`), which `StepForward` calls (the boot's
/// v2-gravel-impreza03-24 run). `tests::code_stand_ins_match_the_exe` checks them against the exe's code.
pub(super) const CODE_STAND_INS: [(u32, StandIn); 31] = [
    (0x499e60, StandIn::Ret(4)),
    (0x499e70, StandIn::Ret(4)),
    (0x499e90, StandIn::Ret(0)),
    (0x4ac490, StandIn::Ret(4)),
    (0x4ac790, StandIn::Ret(4)),
    (0x4e5830, StandIn::Ret(0)),
    (0x503aa0, StandIn::Ret(0)),
    (0x50d570, StandIn::Ret(4)),
    (0x519b80, StandIn::Ret(0)),
    (0x572ff0, StandIn::Ret(4)),
    (0x519e10, StandIn::Ret(0x18)),
    (0x519dd0, StandIn::Ret(0x20)),
    (0x519bc0, StandIn::Ret(4)),
    (0x47aa90, INVOKE),
    (0x491b10, INVOKE),
    (0x49b6e0, INVOKE),
    (0x4af5d0, INVOKE),
    (0x4af6e0, INVOKE),
    (0x4af900, INVOKE),
    (0x4d7900, INVOKE),
    (0x57a6e0, INVOKE),
    (0x57a740, INVOKE),
    (0x5e3c70, INVOKE),
    (0x5e3fd0, INVOKE),
    (0x5f7b50, INVOKE),
    (0x5fec70, INVOKE),
    (0x5fece0, INVOKE),
    (0x61f620, INVOKE),
    (0x61f740, INVOKE),
    (0x4ea830, VC_INVOKE),
    (0x5e5d10, VC_INVOKE),
];
const INVOKE: StandIn = StandIn::Invoke { method: 0x14, owner_offset: None };
const VC_INVOKE: StandIn = StandIn::Invoke { method: 0x18, owner_offset: Some(0x1c) };

/// The pop of the DLL listener whose entry is `addr` with the first bytes `code`, if it is one.
pub(super) fn dll_listener(code: &[u8], addr: u32) -> Option<u32> {
    DLL_LISTENERS.iter().find_map(|&(bytes, at, pop)| {
        let mut code = code.get(..bytes.len())?.to_vec();
        if let Some(at) = at {
            let word = u32::from_le_bytes(code[at..at + 4].try_into().unwrap()).wrapping_sub(addr);
            code[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        (code == bytes).then_some(pop)
    })
}

/// Code outside the bindings that the pure host runs as its known effect. None of it is in `bindings()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandIn {
    /// `CConnection<..>::Invoke`, `mov eax, ecx; mov ecx, [eax + 0x10]; jmp [eax + 0x14]`: the method at
    /// `[conn + method]` on the owner `[conn + 0x10]`, with the same stack. The `VCCConn` form adds `[conn + 0x1c]` to
    /// the owner and jumps through +0x18.
    Invoke {
        method: u32,
        owner_offset: Option<u32>,
    },
    /// `ret n`.
    Ret(u32),
    /// A listener in [`LISTENERS`]: returns at once, popping its arguments.
    Listener(u32),
    /// The CRT sine and cosine wrappers, `fld [esp + 4]; fsin` (`fcos`); `ret`: `math::trig`'s model of the x87.
    Sin,
    Cos,
    /// The CRT's `eh vector constructor iterator` and `eh vector destructor iterator`.
    VectorCtor,
    VectorDtor,
    /// `QueryPerformanceCounter` (through the import table).
    Counter,
    /// The executable's CRT `operator new` and its `new[]` jump, and NGP's `operator new`, modeled by the pure-only
    /// allocator.
    Allocate,
    /// The executable's scalar and array `operator delete`, returning pure-only blocks to their size list.
    Free,
}

/// The stand-in at `addr`, by its code bytes or its address.
pub fn stand_in(mem: &Mem, addr: u32) -> Option<StandIn> {
    // A listener returns at its entry whatever its bytes: the damage receivers are Invoke trampolines.
    if let Some(&(_, pop)) = LISTENERS.iter().find(|&&(at, _)| at == addr) {
        return Some(StandIn::Listener(pop));
    }
    // The longest DLL_LISTENERS key is 55 bytes. An image without the exe's code (`rbr-standalone --boot`) has the
    // code stand-ins by address.
    let Some(code) = mem.try_read(addr, 64) else {
        return CODE_STAND_INS.iter().find_map(|&(at, s)| (at == addr).then_some(s)).or_else(|| by_address(mem, addr));
    };
    code_stand_in(&code).or_else(|| dll_listener(&code, addr).map(StandIn::Listener).or_else(|| by_address(mem, addr)))
}

/// The stand-in `code` (at least 11 bytes) is by itself: an `Invoke` trampoline or a `ret n`.
pub(super) fn code_stand_in(code: &[u8]) -> Option<StandIn> {
    if code[..8] == [0x8b, 0xc1, 0x8b, 0x48, 0x10, 0xff, 0x60, 0x14] {
        return Some(StandIn::Invoke { method: 0x14, owner_offset: None });
    }
    if code[..11] == [0x8b, 0xc1, 0x8b, 0x48, 0x1c, 0x03, 0x48, 0x10, 0xff, 0x60, 0x18] {
        return Some(StandIn::Invoke { method: 0x18, owner_offset: Some(0x1c) });
    }
    match code[0] {
        0xc3 => Some(StandIn::Ret(0)),
        0xc2 => Some(StandIn::Ret(u32::from(code[1]) | u32::from(code[2]) << 8)),
        _ => None,
    }
}

/// The stand-ins known by their address: the CRT's and `QueryPerformanceCounter`.
fn by_address(mem: &Mem, addr: u32) -> Option<StandIn> {
    Some(match addr {
        CRT_NEW | CRT_NEW_ARRAY => StandIn::Allocate,
        CRT_DELETE | CRT_DELETE_ARRAY => StandIn::Free,
        CRT_SIN => StandIn::Sin,
        CRT_COS => StandIn::Cos,
        VECTOR_CTOR => StandIn::VectorCtor,
        VECTOR_DTOR => StandIn::VectorDtor,
        _ if addr == mem.u32(QUERY_PERFORMANCE_COUNTER) => StandIn::Counter,
        _ if Some(addr) == crate::ngp::at(crate::ngp::OPERATOR_NEW) => StandIn::Allocate,
        _ => return None,
    })
}

/// Where a pure host would stop for a call of `addr` with `ecx` and `args`: the function itself, or the method its
/// `Invoke` trampolines lead to, or the element function a vector iterator calls, when that has no binding and no
/// stand-in. `None` when the pure host covers the call (its callees aside). For the emulator's host calls (`ffi`): the
/// pure step's worklist.
pub fn uncovered(mem: &Mem, addr: u32, ecx: u32, args: &[u32]) -> Option<u32> {
    if binding(crate::ngp::file(addr)).is_some() {
        return None;
    }
    if let Some(exe) = crate::ngp::original_for(addr) {
        return uncovered(mem, exe, ecx, args);
    }
    match stand_in(mem, addr) {
        Some(StandIn::Invoke { method, owner_offset }) => {
            let offset = owner_offset.map_or(0, |at| mem.u32(ecx + at));
            uncovered(mem, mem.u32(ecx + method), mem.u32(ecx + 0x10).wrapping_add(offset), args)
        }
        Some(StandIn::VectorCtor | StandIn::VectorDtor) => match *args {
            [ptr, _, count, element, ..] if count as i32 > 0 => uncovered(mem, element, ptr, &[]),
            _ => None,
        },
        Some(_) => None,
        None => Some(addr),
    }
}
