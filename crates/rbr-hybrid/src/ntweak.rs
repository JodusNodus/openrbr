//! Load-time bindings: `NTweak::CBase::Parse` (0x52f0c0) on the token tables (`rbr_physics::ntweak::parse`), with
//! `CTokenCursor::Tokenize` (0x52ec80) and the token heap's `E11_StackAlloc` (0x473a70). They run from load-time entry
//! dumps (`RBR_SNAPAT`), not from step snapshots. The calls `Parse` leaves to the objects (an `Unk14` other than
//! `CBase`'s, a member object's `Parse`, slot 17) and `operator new`/`delete` run in the emulator at the original's
//! stack.

use rbr_formats::lsp_keys::Class;
use rbr_physics::mem::Mem;
use rbr_physics::ntweak::{self, Frame, ParseHost};

use crate::pure::CRT_DELETE;
use crate::rigid_body::OPERATOR_NEW;
use crate::{Binding, Ctx, Decline, Host, Regs, Return};

pub static BINDINGS: [Binding; 6] = [
    Binding { addr: 0x52f450, name: "NTweak::CBase::SetDirty", subsystem: "ntweak", run: set_dirty },
    Binding { addr: 0x52f090, name: "NTweak::CBase::CBase", subsystem: "ntweak", run: construct_base },
    Binding { addr: 0x52f480, name: "NTweak::CBase::Unk44", subsystem: "ntweak", run: crate::entities::empty_hook_run },
    Binding { addr: 0x52f0c0, name: "NTweak::CBase::Parse", subsystem: "ntweak", run: parse },
    Binding { addr: 0x52ec80, name: "NTweak::CTokenCursor::Tokenize", subsystem: "ntweak", run: tokenize },
    Binding { addr: 0x473a70, name: "E11_StackAlloc", subsystem: "ntweak", run: stack_alloc },
];

/// The classes `Parse` walks, by vtable (each is a vtable whose slot 2 is `Parse`, named by its RTTI), as the key table
/// their slot 8 (`Unk20`, a `mov eax, dict`) binds: `data/physics-lsp-fields.md`. `CCarLightSystem::CState`'s returns
/// the engine state's dictionary, `CSolid::CState`'s and `CWheel::CState`'s the shared entity-state table. Not here:
/// `NTweak::CBase`, `CBody::CDef` and `CEntity::CDef` (abstract), `CEntity::CState` (no dictionary), and the season's
/// `CSeasonRallySettings` and `CSeasonStageWeather` (no key table ported).
const CLASSES: [(u32, Class); 49] = [
    (0x72e494, Class::CarSetup),
    (0x72e4e4, Class::EngineSetup),
    (0x72e534, Class::VcuSetup),
    (0x72e584, Class::WheelSetup),
    (0x72e5d4, Class::SpringSetup),
    (0x72e624, Class::TyreSetup),
    (0x72e674, Class::TyreTypes),
    (0x72e724, Class::CarSetupOptions),
    (0x72e774, Class::DriveSetupOptions),
    (0x72e7c4, Class::VcuSetupOptions),
    (0x72e814, Class::EngineSetupOptions),
    (0x72e864, Class::SpringSetupOptions),
    (0x72e8b4, Class::TyreSetupOptions),
    (0x72e904, Class::WheelSetupOptions),
    (0x72ec5c, Class::DriveSetup),
    (0x72ecac, Class::CompleteCarSetupOptions),
    (0x72ed24, Class::CompleteCarSetup),
    (0x737c34, Class::BodyState),
    (0x73823c, Class::CarState),
    (0x7382a4, Class::CarDef),
    (0x738ddc, Class::CarBodyDef),
    (0x738e4c, Class::CarBodyState),
    (0x739264, Class::CarLightDef),
    (0x7392bc, Class::EngineState),
    (0x7398f4, Class::CollisionSolverDef),
    (0x739e44, Class::DefManager),
    (0x73a0cc, Class::DriveState),
    (0x73a14c, Class::DriveDef),
    (0x73b5dc, Class::EngineState),
    (0x73b694, Class::EngineDef),
    (0x73ceac, Class::GroundDef),
    (0x73d1ac, Class::HingedDef),
    (0x73d81c, Class::Material),
    (0x73d86c, Class::MaterialManager),
    (0x73e59c, Class::MountedDef),
    (0x73e7d4, Class::PassengerDef),
    (0x73e834, Class::PassengerState),
    (0x73f72c, Class::PhysDef),
    (0x73f884, Class::PhysState),
    (0x73fafc, Class::SolidDef),
    (0x73fc04, Class::EntityState),
    (0x740124, Class::SpringDef),
    (0x74018c, Class::SpringState),
    (0x740b04, Class::TyreDef),
    (0x740b7c, Class::TyreState),
    (0x7412dc, Class::VcuDef),
    (0x74132c, Class::VcuState),
    (0x7427b4, Class::EntityState),
    (0x74281c, Class::WheelDef),
];

/// The definition registry (`CDefManager`'s dictionary) and `CDictBase::Find(name, &index)` on it.
const REGISTRY: u32 = 0x7c8040;
const DICT_FIND: u32 = 0x52ebf0;
/// The objects' slot 5 (`Unk14`), a member's slot 2 (`Parse`) and slot 17 (`Unk44`).
const SLOT_UNK14: u32 = 0x14;
const SLOT_PARSE: u32 = 0x08;
const SLOT_AFTER_PARSE: u32 = 0x44;

/// The calls `Parse` makes, entered where the original makes them. Its frame is 8-aligned below the pushed `ebp`
/// (`base`); its cursors are at `base - 0x20` and `base - 0x18`.
struct Original<'a> {
    host: &'a dyn Host,
    base: u32,
}

impl Original<'_> {
    fn frame(&self) -> Frame {
        Frame { tokens: self.base - 0x20, inner: self.base - 0x18 }
    }
}

impl ParseHost for Original<'_> {
    /// `operator new` in `Tokenize`, called with three pushes and the argument below its entry.
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.host.call_at(self.base - 0x58, OPERATOR_NEW, 0, &[size]).eax
    }

    fn free(&mut self, _: &mut Mem, at: u32) {
        self.host.call_at(self.base - 0x38, CRT_DELETE, 0, &[at]);
    }

    fn unk14(&mut self, mem: &mut Mem, this: u32, name: u32, block: u32, cursor: u32) {
        let slot = mem.u32(mem.u32(this) + SLOT_UNK14);
        self.host.call_at(self.base - 0x40, slot, this, &[name, block, cursor]);
    }

    fn member_parse(&mut self, mem: &mut Mem, member: u32, cursor: u32) {
        let slot = mem.u32(mem.u32(member) + SLOT_PARSE);
        self.host.call_at(self.base - 0x38, slot, member, &[cursor]);
    }

    fn after_parse(&mut self, mem: &mut Mem, this: u32) {
        let slot = mem.u32(mem.u32(this) + SLOT_AFTER_PARSE);
        self.host.call_at(self.base - 0x34, slot, this, &[]);
    }

    /// `Find` (the registry's slot 5) on the name copied to the free stack (the original passes the cut token), its
    /// index into the original's local at `base - 0x24`.
    fn def_index(&mut self, mem: &mut Mem, name: &str) -> Option<u32> {
        let at = self.base - 0x400;
        mem.map(at, &[name.as_bytes(), &[0]].concat());
        let index = self.base - 0x24;
        let found = self.host.call_at(self.base - 0x3c, DICT_FIND, REGISTRY, &[at, index]).eax;
        (found != 0).then(|| mem.u32(index))
    }
}

/// thiscall `(cursor*)`, eax 0: the walk over the group at the cursor's token.
fn parse(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let (this, cursor) = (r.ecx, r.stack[0]);
    let vtable = ctx.mem.u32(this);
    let Some(&(_, class)) = CLASSES.iter().find(|(v, _)| *v == vtable) else {
        return Err(Decline::Unsupported("an object of a class whose key table is not ported"));
    };
    let mut host = Original { host: ctx.host, base: (r.esp - 4) & !7 };
    let frame = host.frame();
    ntweak::parse(&mut ctx.mem, &mut host, crate::ngp::variant(), frame, this, class, cursor)
        .map_err(|e| Decline::Unsupported(e.0))?;
    Ok(Return::void(0, 4))
}

/// thiscall `(text, 0)`, eax the cursor: its table of the text's tokens, with `operator new` entered below the four
/// pushes, `edi` and the argument.
fn tokenize(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let sp = r.esp - 0x1c;
    let host = ctx.host;
    ntweak::tokenize(&mut ctx.mem, |_, size| host.call_at(sp, OPERATOR_NEW, 0, &[size]).eax, r.ecx, r.stack[0])
        .map_err(|e| Decline::Unsupported(e.0))?;
    Ok(Return::void(r.ecx, 8))
}

/// cdecl `(size, heap*, align)`, eax the block.
fn stack_alloc(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [size, heap, align, ..] = r.stack;
    let block = ntweak::stack_alloc(&mut ctx.mem, size, heap, align).map_err(|e| Decline::Unsupported(e.0))?;
    Ok(Return::void(block, 0))
}

/// thiscall `()`: `NTweak::CBase::SetDirty`; eax is the caller's.
fn set_dirty(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ntweak::set_dirty(&mut ctx.mem, r.ecx);
    Ok(Return::void(r.eax, 0))
}

/// thiscall `(name)`: `NTweak::CBase::CBase`, eax is `this`.
fn construct_base(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ntweak::construct_base(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(r.ecx, 4))
}
