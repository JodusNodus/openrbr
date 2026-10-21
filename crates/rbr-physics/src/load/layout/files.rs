//! The definitions a physics file makes (the factories' definitions, the tweak walk with the registry answered) and
//! the physics files' root groups.

use rbr_formats::lsp::Node;
use rbr_formats::lsp_keys::Class;

use crate::load::Registry;
use crate::mem::Mem;
use crate::ntweak::{self, ApplyHost, Unsupported};
use crate::physics_core::layout::construct::{self, Heap};
use crate::physics_core::layout::mounted_ctor::{self, MountedHost};
use crate::physics_core::layout::setup::CDEF_SIZE;
use crate::{car_body, entities, rigid_body, wheel};

/// The mounted definition's calls, answered by the ports: the solid definition's constructor and the unsigned's.
pub(super) struct Mounted<'a, H>(pub(super) &'a mut H);

impl<H: Heap> Heap for Mounted<'_, H> {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        self.0.alloc(mem, size)
    }
}

impl<H> MountedHost for Mounted<'_, H> {
    fn solid_def(&mut self, mem: &mut Mem, at: u32, name: u32) {
        rigid_body::layout::solid_def_ctor(mem, at, name);
    }

    fn unsigned(&mut self, mem: &mut Mem, at: u32) {
        ntweak::construct_unsigned(mem, at);
    }
}

/// A factory's definition, built on the registry's name block `outer`; `None` for a factory the files do not use.
fn make_def(mem: &mut Mem, host: &mut impl Heap, factory: &str, outer: u32) -> Option<u32> {
    match factory {
        // `CSolid::NewDef` (0x52b8b0)
        "Solid" => Some(rigid_body::layout::new_def(mem, outer, host)),
        // `CGround::CDef::New9` (0x51baa0)
        "Ground" => {
            let at = host.alloc(mem, entities::layout::GROUND_DEF_SIZE);
            if at != 0 {
                entities::layout::construct_ground_def(mem, at, outer);
            }
            Some(at)
        }
        // `CWheel::M3B610` (0x53b610) and `CCar::CDef::Create` (0x4e6f20)
        "Wheel" => Some(wheel::layout::new_cdef(mem, outer, host)),
        "Car" => Some(car_body::layout::new_def(mem, outer, host)),
        "CarBody" => Some(car_body::layout::new_body_def(mem, outer, host)),
        "HingedBody" => Some(entities::layout::hinged_def_new(mem, outer, host)),
        "MountedBody" => Some(mounted_ctor::new_def(mem, &mut Mounted(host), outer)),
        _ => None,
    }
}

/// The class of a factory's definitions.
fn def_class(factory: &str) -> Option<Class> {
    match factory {
        "Solid" => Some(Class::SolidDef),
        "Ground" => Some(Class::GroundDef),
        "Wheel" => Some(Class::WheelDef),
        "Car" => Some(Class::CarDef),
        "CarBody" => Some(Class::CarBodyDef),
        "HingedBody" => Some(Class::HingedDef),
        "MountedBody" => Some(Class::MountedDef),
        _ => None,
    }
}

/// A tweak object's vtable slot 17 (`Unk44`), which the walk runs after its group.
const SLOT_17: u32 = 0x44;

/// The walk's calls with the definition registry answered from `registry`.
struct Linked<'a, H> {
    host: &'a mut H,
    registry: &'a mut Registry,
}

impl<H: ApplyHost + Heap> ApplyHost for Linked<'_, H> {
    fn alloc_name(&mut self, mem: &mut Mem, len: u32) -> u32 {
        self.host.alloc_name(mem, len)
    }

    /// The car body definition's slot 17 is ported; the others go to the host.
    fn after_parse(&mut self, mem: &mut Mem, this: u32) {
        match mem.u32(mem.u32(this) + SLOT_17) {
            car_body::layout::BODY_DEF_AFTER_PARSE => car_body::layout::body_def_after_parse(mem, this),
            _ => self.host.after_parse(mem, this),
        }
    }

    fn def_index(&mut self, _: &Mem, name: &str) -> Option<u32> {
        self.registry.index(name)
    }

    /// `CDefManager::Vf14` (0x501ef0): the registry's definition of `name` when `factory` made it, else a new one from
    /// the factory (which takes the name's place), then the group walked on it. A factory the files do not use
    /// registers the name with no definition (0), and its group is not walked.
    fn define(
        &mut self,
        mem: &mut Mem,
        factory: &str,
        block: u32,
        name: &str,
        group: &[Node],
    ) -> Result<(), Unsupported> {
        let def = match self.registry.def(factory, name) {
            Some(def) => def,
            None => {
                let def = make_def(mem, self.host, factory, block).unwrap_or(0);
                self.registry.set(factory, name, def);
                def
            }
        };
        match def_class(factory).filter(|_| def != 0) {
            Some(class) => ntweak::apply(mem, self, def, class, group),
            None => Ok(()),
        }
    }
}

/// `E46_CreateDef`'s block (0x526660) with `CPhysics::CDef::CDef` (0x524c70) on it, its members built by their ports.
/// Returns the definition (0 when the allocation fails).
pub fn new_physics_def(mem: &mut Mem, host: &mut impl Heap) -> u32 {
    let def = host.alloc(mem, CDEF_SIZE);
    if def != 0 {
        construct::construct_cdef(mem, &mut crate::physics_core::layout::Ported, def);
    }
    def
}

/// A physics file's root group (`prime.lsp`, `physics.lsp`, the car's `common.lsp` and surface file) walked on the
/// physics definition at `def`, as `E46_InitPhysics` (0x5266d0) hands it to the definition's `Parse`: its definition
/// manager's groups make or find each definition in `registry` and walk its group on it.
pub fn physics_file(
    mem: &mut Mem,
    host: &mut (impl ApplyHost + Heap),
    registry: &mut Registry,
    def: u32,
    root: &[Node],
) -> Result<(), Unsupported> {
    let mut linked = Linked { host, registry };
    ntweak::apply(mem, &mut linked, def, Class::PhysDef, root)
}
