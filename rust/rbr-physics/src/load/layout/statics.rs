//! The `.data` the step reads before the physics load writes it: the statics, the car meshes, the profiler sample,
//! the replay stand-in, the signal manager and the registry.

use crate::load::car_grids::{CarMesh, SampleMap};
use crate::load::Registry;
use crate::mem::Mem;
use crate::physics_core::construct::Heap;
use crate::physics_core::layout::{REGISTRY, TELEMETRY_GLOBAL};
use crate::physics_core::prof::EnterHost;
use crate::{collision, physics_core};

/// The block a static initializer (0x7239d0) fills: a cleared vector, then three `.rdata` constants (1.0, 2.2 and 0.75),
/// which `E36Collect` (0x4f3950) reads.
const COLLECT_STATICS: u32 = 0x007c_7050;
const COLLECT_CONSTANTS: [u32; 3] = [0x0079_5e14, 0x0073_9488, 0x0072_f448];
/// The collision mesh parameters `E02FireCollisionMesh` (0x440980) stores and hands to the animation objects' meshes:
/// two words 0, then 1.0 at +8 and 0.3 at +0x10 (`.rdata` constants). `E33Mat34::ComposeA` and `E36Collect` read them
/// through the meshes.
const MESH_PARAMS: u32 = 0x007e_a250;
const MESH_PARAM_CONSTANTS: [(u32, u32); 2] = [(8, 0x0079_5e14), (0x10, 0x0072_afcc)];

/// The `.data` constants the step reads that code before the physics load writes: `COLLECT_STATICS` (0x7239d0) and the
/// collision mesh parameters (`E02FireCollisionMesh`'s stores). `mem` holds the exe's `.rdata`.
pub fn store_statics(mem: &mut Mem) {
    mem.map(COLLECT_STATICS, &[0; 16]);
    for (k, at) in COLLECT_CONSTANTS.into_iter().enumerate() {
        let value = mem.u32(at);
        mem.set_u32(COLLECT_STATICS + 0x20 + 4 * k as u32, value);
    }
    mem.set_u32(MESH_PARAMS, 0);
    mem.set_u32(MESH_PARAMS + 4, 0);
    for (offset, at) in MESH_PARAM_CONSTANTS {
        let value = mem.u32(at);
        mem.set_u32(MESH_PARAMS + offset, value);
    }
}

/// A `CSigCollisionMesh` (0x34 bytes): `collision::layout::load_mesh_desc` has the fields.
const MESH_DESC_SIZE: u32 = 0x34;
/// `E02FireCollisionMesh`'s mesh: grid 9, a sphere (flag 0x10000) named `"animationobject"` (0x72afbc) with the shape
/// record `MESH_PARAMS` and material 200.
const FIRE_MESH_ID: u32 = 9;
const FIRE_MESH_FLAGS: u32 = 0x1_0000;
const FIRE_MESH_NAME: u32 = 0x0072_afbc;
const FIRE_MESH_MATERIAL: u8 = 200;

/// The car loader's meshes (0x1001ecc0, `car_grids::car_meshes`), each emitted to `E37SigFactory720`, which builds its
/// collision grid. HedgeHog3D is at `hh_base`. The heap gives, in this order: the body's maps (the file's first, then
/// its second; the file loader's), then per mesh its descriptor, vertices (16 bytes each, `w` not written), faces (7
/// bytes each) and the body's samples. Returns the descriptors.
pub fn emit_car_meshes(mem: &mut Mem, host: &mut impl Heap, hh_base: u32, meshes: &[CarMesh]) -> Vec<u32> {
    let maps: Vec<[u32; 2]> = meshes
        .iter()
        .map(|m| {
            let mut data = |map: &SampleMap| map.data.as_ref().map_or(0, |d| put(mem, host, d));
            let b = data(&m.map_b);
            [data(&m.map_a), b]
        })
        .collect();
    let mut descs = Vec::new();
    for (m, [map_a, map_b]) in meshes.iter().zip(maps) {
        let desc = put(mem, host, &[0; MESH_DESC_SIZE as usize]);
        let vertices = host.alloc(mem, 16 * m.vertices.len() as u32);
        for (k, v) in m.vertices.iter().enumerate() {
            for (lane, &x) in v.iter().enumerate() {
                mem.set_f32(vertices + 16 * k as u32 + 4 * lane as u32, x);
            }
        }
        let faces = put(mem, host, m.faces.as_flattened());
        mem.set_u32(desc, m.id);
        mem.set_u32(desc + 8, hh_base + m.name);
        mem.map(desc + 0xc, &(m.vertices.len() as u16).to_le_bytes());
        mem.map(desc + 0xe, &(m.faces.len() as u16).to_le_bytes());
        mem.set_u32(desc + 0x10, vertices);
        mem.set_u32(desc + 0x14, faces);
        if let Some(samples) = &m.samples {
            mem.set_u32(desc + 0x18, samples.len() as u32);
            let data = put(mem, host, samples.as_flattened());
            mem.set_u32(desc + 0x1c, data);
            mem.map(desc + 0x20, &m.map_a.size.to_le_bytes());
            mem.map(desc + 0x22, &m.map_b.size.to_le_bytes());
            mem.set_u32(desc + 0x24, map_a);
            mem.set_u32(desc + 0x28, map_b);
        }
        collision::layout::collision_mesh_factory(mem, desc);
        descs.push(desc);
    }
    descs
}

/// A heap block holding `bytes`.
fn put(mem: &mut Mem, host: &mut impl Heap, bytes: &[u8]) -> u32 {
    let at = host.alloc(mem, bytes.len() as u32);
    mem.map(at, bytes);
    at
}

/// `E02FireCollisionMesh` (0x440980)'s emit to `E37SigFactory720`: grid 9 from its descriptor, a stack temporary the
/// caller places at `desc`.
pub fn fire_collision_mesh(mem: &mut Mem, desc: u32) {
    mem.map(desc, &[0; MESH_DESC_SIZE as usize]);
    mem.set_u32(desc, FIRE_MESH_ID);
    mem.set_u32(desc + 4, FIRE_MESH_FLAGS);
    mem.set_u32(desc + 8, FIRE_MESH_NAME);
    mem.set_u32(desc + 0x2c, MESH_PARAMS);
    mem.set_u8(desc + 0x30, FIRE_MESH_MATERIAL);
    collision::layout::collision_mesh_factory(mem, desc);
}

/// `CApplication::Tic`'s profiler sample (0x7eacc0) and its name (0x72eff4).
const TIC_SAMPLE: u32 = 0x007e_acc0;
const TIC_NAME: u32 = 0x0072_eff4;

/// The profiler's queue as the frame's `CApplication::Tic` (0x4804f0) leaves it before it calls the physics: its sample
/// entered (`E46ProfChan::Enter`, the sample alone in the queue).
pub fn enter_tic_sample(mem: &mut Mem, host: &mut impl EnterHost) {
    physics_core::layout::prof_enter(host, mem, TIC_SAMPLE, TIC_NAME);
}

/// The replay manager's global (`g_e19Ptr893634`) and its state word, which `E19IsState2` (0x499900) tests for 2 under
/// `CCar::UpdateSeed`; `CReplayManager::Ctor` (0x49b800) stores 3 there (no replay running).
const REPLAY_STATE: u32 = 0x10;
const REPLAY_IDLE: u32 = 3;
/// The stand-in's size: the words up to the state.
pub const REPLAY_STAND_IN_SIZE: u32 = 0x14;

/// The replay manager as the step sees it. The standalone has no replay (decision O1), so this is a stand-in block that
/// holds only the constructor's state word (idle), and the global points at it. `StepForward` also passes the global to
/// the replay's listeners, which the pure host skips.
pub fn new_replay_stand_in(mem: &mut Mem, host: &mut impl Heap) -> u32 {
    let at = host.alloc(mem, REPLAY_STAND_IN_SIZE);
    mem.set_u32(at + REPLAY_STATE, REPLAY_IDLE);
    mem.set_u32(TELEMETRY_GLOBAL, at);
    at
}

/// `CSignalManager::CSignalManager(state)` (0x465260, from `CPC_Application`'s constructor 0x481290 with the
/// application's state word, `+0x728`): its 127 `CMultiSignal1` members, 0x218 bytes apart, each made by the same
/// inline constructor (0x4633d0's first part): the busy word 0, the current type's pointer `state` (+4), the 32 lists
/// by type (+8, 0x10 each, `0x454930`: four zero words) and the list for every type (+0x208, zero). On a zeroed block
/// only the state pointers are written.
pub fn construct_signal_manager(mem: &mut Mem, at: u32, state: u32) {
    for k in 0..SIGNAL_COUNT {
        let signal = at + SIGNAL_SIZE * k;
        mem.map(signal, &[0; SIGNAL_SIZE as usize]);
        mem.set_u32(signal + 4, state);
    }
}

/// The signal manager's members and their size: the block `CPC_Application` allocates (0x109e8).
pub const SIGNAL_COUNT: u32 = 127;
pub const SIGNAL_SIZE: u32 = 0x218;

/// The definition registry's count (`+8`) and its entries (`+0x1c`, 0x14 bytes each: the definition, then the name's
/// token-heap block).
const REGISTRY_COUNT: u32 = 8;
const REGISTRY_ENTRIES: u32 = 0x1C;
const REGISTRY_ENTRY: u32 = 0x14;

/// The registry (`CDefFactory` at 0x7c8040) in memory as `CDefManager::Vf14` (0x501ef0) leaves it: the count and each
/// entry's definition, which `E39_EntfCreate` reads. The name words are not stored (nothing the step or the placements
/// run reads them).
pub fn store_registry(mem: &mut Mem, registry: &Registry) {
    let at = REGISTRY;
    mem.set_u32(at + REGISTRY_COUNT, registry.entries().len() as u32);
    for (k, entry) in registry.entries().iter().enumerate() {
        let entry_at = at + REGISTRY_ENTRIES + REGISTRY_ENTRY * k as u32;
        mem.map(entry_at, &[0; REGISTRY_ENTRY as usize]);
        mem.set_u32(entry_at, entry.def);
    }
}
