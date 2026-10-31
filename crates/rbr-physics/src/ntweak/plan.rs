//! The NTweak walk over a [`TweakStore`]: the group's effects worked out before anything is written (`super` describes
//! the walk, `layout` applies the plan on game memory).

use rbr_formats::lsp::Node;
use rbr_formats::lsp_keys::{Class, KeyInfo};
use rbr_formats::lsp_keys_ngp::ngp_key_info;

use super::store::TweakStore;
use super::{atof, atoi, get_quoted, is_float, is_integer, set_bit, Unsupported};

use crate::physics_core::layout::{construct as physics, flags, material};

/// The members' reader entries, by which the walk tells them apart.
pub const REAL_PARSE: u32 = 0x52f770;
pub const UNSIGNED_PARSE: u32 = 0x52f840;
pub const INTEGER_PARSE: u32 = 0x52fab0;
pub const VECTOR_PARSE: u32 = 0x52fb20;
pub const VECTOR4_PARSE: u32 = 0x52fbe0;
pub const BIT_FIELD_PARSE: u32 = 0x52f930;
/// The base parse itself: a member object with keys of its own.
pub const BASE_PARSE: u32 = 0x52f0c0;
/// Under NGP, the brake-wear table of the drive's definition: NGP's copy of the unsigned's table with its own reader,
/// which NGP's constructor makes and puts on the unsigned. The walk tells the reader by the table itself.
pub const NGP_TABLE_VTABLE: u32 = 0x1017_6518;
/// The 30 floats NGP's reader takes after the unsigned, into the words from the member's table start (past the 1.02
/// keys, which no 1.02 key reads): three rows of ten.
pub(crate) const NGP_TABLE: u32 = 0x10;
pub(crate) const NGP_TABLE_LEN: usize = 30;
/// The objects' inner-group entries, which take the walk's inner groups: the base's skips each item; the entity
/// definitions' links the group's name; the definition manager's makes or finds the definition the item names and walks
/// the item on it.
pub(crate) const UNK14_SKIP: u32 = 0x52fe80;
pub(crate) const UNK14_LINK: u32 = 0x5198b0;
pub(crate) const UNK14_DEFINE: u32 = 0x501ef0;
/// The token-heap block each item of an inner group is copied into.
pub(crate) const ITEM_BLOCK: u32 = 0x20;
/// An entity definition's links: their count, then the registry indices.
pub(crate) const LINK_COUNT: u32 = 0x18;
pub(crate) const LINKS: u32 = 0x1c;

/// A tweak value's payload, after its base header (table, name, dirty flag); a vector's is 16-byte aligned. A
/// bit-field's mask is its value.
pub(crate) const VALUE: u32 = 0x0c;
pub(crate) const VECTOR_VALUE: u32 = 0x10;

/// The bit-field classes by table: their names and bit counts.
pub(crate) const BIT_FIELDS: [(u32, u32, u32); 4] = [
    (physics::FLAGS_VTABLE, flags::CDEF_NAMES, flags::CDEF_BITS),
    (physics::DBG_FLAGS_VTABLE, flags::DBG_NAMES, flags::DBG_BITS),
    (physics::LOG_FLAGS_VTABLE, flags::LOG_NAMES, flags::LOG_BITS),
    (material::FLAGS_VTABLE, flags::MATERIAL_NAMES, flags::MATERIAL_BITS),
];

fn atom(item: Option<&Node>) -> Option<&str> {
    match item {
        Some(Node::Atom(t)) => Some(t),
        _ => None,
    }
}

/// One effect of the walk, in the original's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Action<'a> {
    /// A copy of the name on the token heap; the object points at it.
    Name(u32, String),
    /// A member's value word (address, value).
    Store(u32, u32),
    /// A token-heap block for an inner group's item, holding what `GetQuoted` takes from the item.
    Item(String),
    /// The registry index of the name, appended to the object's links when the registry has the name.
    Link(u32, String),
    /// The definition manager's inner-group entry on an item: the factory, the definition's name and the item's group.
    /// The name's block is the last item's.
    Define(&'a str, String, &'a [Node]),
    /// The object's post-walk entry.
    After(u32),
}

/// The floats from item `i` on, up to `n`, as words from `at`: what a real, vector or four-vector reader takes.
/// Returns the item after the last one taken.
fn floats(items: &[Node], i: usize, n: usize, at: u32, out: &mut Vec<Action>) -> usize {
    let taken = (0..n)
        .map_while(|k| atom(items.get(i + k)).filter(|v| is_float(v)))
        .enumerate()
        .map(|(k, v)| Action::Store(at + 4 * k as u32, atof(v).to_bits()));
    let before = out.len();
    out.extend(taken);
    i + out.len() - before
}

/// NGP's reader on the unsigned at `member`: the unsigned parse, whose result it ignores, then up to
/// [`NGP_TABLE_LEN`] floats into the table, stopping at the first item that is not one. Returns the item after the last
/// one taken.
fn ngp_table(items: &[Node], i: usize, member: u32, out: &mut Vec<Action>) -> usize {
    let i = match atom(items.get(i)).filter(|v| is_integer(v)) {
        Some(v) => {
            out.push(Action::Store(member + VALUE, atoi(v)));
            i + 1
        }
        None => i,
    };
    floats(items, i, NGP_TABLE_LEN, member + NGP_TABLE, out)
}

/// The word at `at` as the walk has left it so far: its last store, else memory's.
fn current<S: TweakStore>(store: &S, out: &[Action], at: u32) -> u32 {
    out.iter()
        .rev()
        .find_map(|a| match a {
            Action::Store(to, value) if *to == at => Some(*value),
            _ => None,
        })
        .unwrap_or_else(|| store.word(at))
}

/// The bit-field at `member`: the items from `i` on that start with `+` or `-`, each setting or clearing its bit in
/// the mask. Returns the item after the last one taken.
fn bit_field<S: TweakStore>(
    store: &S,
    member: u32,
    items: &[Node],
    i: usize,
    out: &mut Vec<Action>,
) -> Result<usize, Unsupported> {
    let vtable = store.vtable(member);
    let Some(&(_, table, count)) = BIT_FIELDS.iter().find(|(v, _, _)| *v == vtable) else {
        return Err(Unsupported("a bit-field of a class not ported"));
    };
    let names = store.bit_names(table, count);
    let at = member + VALUE;
    let mut mask = current(store, out, at);
    let mut next = i;
    while let Some(item) = atom(items.get(next)).filter(|t| t.starts_with(['+', '-'])) {
        mask = set_bit(mask, &names, item);
        next += 1;
    }
    if next > i {
        out.push(Action::Store(at, mask));
    }
    Ok(next)
}

/// The key record of `key` in `class` from NGP's tables (its renamed and appended keys, `notes/ngp.md` "Load path").
pub(super) fn lookup(class: Class, key: &str) -> Option<KeyInfo> {
    ngp_key_info(class, key)
}

/// The member a key names in the object at `this`: at its offset, or in the material manager the material of its tag
/// (the manager's key words are `88 * tag + 3`; the key tables hold the tag).
pub(super) fn member_at(this: u32, info: &KeyInfo) -> u32 {
    match info.class {
        Class::MaterialManager => material::entry(this, info.offset),
        _ => this + info.offset,
    }
}

/// One step of the walk at item `i`: the item to go on at, or `None` when the walk ends.
fn step<'a, S: TweakStore>(
    store: &S,
    this: u32,
    class: Class,
    items: &'a [Node],
    i: usize,
    out: &mut Vec<Action<'a>>,
) -> Result<Option<usize>, Unsupported> {
    let t = match &items[i] {
        Node::Atom(t) => t,
        Node::Quoted(_) => return Ok(Some(i + 1)),
        Node::List(group) => {
            inner_group(store, this, group, out)?;
            return Ok(Some(i + 1));
        }
    };
    let Some(info) = (!is_float(t)).then(|| lookup(class, t)).flatten() else {
        return Ok(Some(i + 1));
    };
    let i = i + 1;
    if i == items.len() {
        return Ok(None);
    }
    let member = member_at(this, &info);
    if store.vtable(member) == NGP_TABLE_VTABLE {
        return Ok(Some(ngp_table(items, i, member, out)));
    }
    Ok(Some(match store.reader(store.vtable(member)) {
        REAL_PARSE => floats(items, i, 1, member + VALUE, out),
        VECTOR_PARSE => floats(items, i, 3, member + VECTOR_VALUE, out),
        VECTOR4_PARSE => floats(items, i, 4, member + VECTOR_VALUE, out),
        UNSIGNED_PARSE | INTEGER_PARSE => match atom(items.get(i)).filter(|v| is_integer(v)) {
            Some(v) => {
                out.push(Action::Store(member + VALUE, atoi(v)));
                i + 1
            }
            None => i,
        },
        BIT_FIELD_PARSE => bit_field(store, member, items, i, out)?,
        BASE_PARSE => match (&items[i], info.child) {
            (Node::List(group), Some(child)) => {
                walk(store, member, child, group, out)?;
                i + 1
            }
            _ => return Err(Unsupported("a member object without a group of a known class")),
        },
        _ => return Err(Unsupported("a member whose reader is not ported")),
    }))
}

/// A group inside the group, handed to the object's inner-group entry once per item after its leading quoted name, each
/// item's quoted name first copied to a token-heap block. The walk takes a group token only when it holds
/// whitespace; a group of one item may have none, so the walk declines it.
fn inner_group<'a, S: TweakStore>(
    store: &S,
    this: u32,
    group: &'a [Node],
    out: &mut Vec<Action<'a>>,
) -> Result<(), Unsupported> {
    let [Node::Quoted(name), items @ ..] = group else {
        return Err(Unsupported("an inner group without a name"));
    };
    if items.is_empty() {
        return Err(Unsupported("an inner group of one item"));
    }
    let unk14 = store.unk14(this);
    for item in items {
        let quoted = get_quoted(item, ITEM_BLOCK);
        out.push(Action::Item(quoted.clone()));
        match (unk14, item) {
            (UNK14_SKIP, _) => {}
            (UNK14_LINK, _) => out.push(Action::Link(this, name.clone())),
            (UNK14_DEFINE, Node::List(def)) => out.push(Action::Define(name, quoted, def)),
            (UNK14_DEFINE, _) => return Err(Unsupported("a definition that is not a group")),
            _ => return Err(Unsupported("an inner group of an object with its own Unk14")),
        }
    }
    Ok(())
}

/// The walk over one group: the name, the items, then the post-walk entry. A member object's group (a base member,
/// such as a wheel's spring-damper) is walked in place, as its own parse does it.
fn walk<'a, S: TweakStore>(
    store: &S,
    this: u32,
    class: Class,
    items: &'a [Node],
    out: &mut Vec<Action<'a>>,
) -> Result<(), Unsupported> {
    let mut i = match items.first() {
        Some(Node::Quoted(name)) => {
            out.push(Action::Name(this, name.clone()));
            1
        }
        _ => 0,
    };
    while i < items.len() {
        match step(store, this, class, items, i, out)? {
            Some(next) => i = next,
            None => break,
        }
    }
    out.push(Action::After(this));
    Ok(())
}

/// What a group does to its object and its member objects, worked out before anything is written. A definition the
/// definition manager makes is walked when the plan is applied, as it does not exist before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan<'a> {
    pub(super) actions: Vec<Action<'a>>,
}

/// The walk over the group, or `Unsupported` for a group it does not cover.
pub fn plan<'a, S: TweakStore>(store: &S, this: u32, class: Class, items: &'a [Node]) -> Result<Plan<'a>, Unsupported> {
    let mut actions = Vec::new();
    walk(store, this, class, items, &mut actions)?;
    Ok(Plan { actions })
}
