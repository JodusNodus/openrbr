//! The E43 pair walkers (src/rbr/RBR_E43_MatrixA.cpp, MatrixB.cpp and MatrixC.cpp). A walker takes a node
//! (`receiver`) and another node `o2` of a second tree, and walks the two in step: it calls a frame hook
//! (`Hook`, vtable slots 1..6) on each node and walks their children (`+4` links). Which hooks and which child
//! walker depend on whether the receiver is deeper or shallower than `o2` (their depths, `+8`). Every walker
//! returns its `a`.
//!
//! The port calls hooks and child walks through `Walk`; the caller supplies the node reads and the calls.

use super::frame::Hook;

/// `E43PairWalk::MAB10`, `MAB20`, `MAB30` and `MAB40` (0x51ab10..0x51ab40): return the first stack argument;
/// `this` and the second argument are unused.
pub fn return_first(_this: u32, a: u32, _b: u32) -> u32 {
    a
}

/// A node of a walked tree: the child link (`+4`) and the depth (`+8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub child: u32,
    pub depth: u32,
}

/// The walkers ported so far. Each name gives the hooks it calls and the depth relation it handles; the C++
/// name and address are in the comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walker {
    /// `E43PairWalk::W9D0` (0x51a9d0): hooks One and Two, no depth test.
    PairOneTwo,
    /// `E43PairWalk::WA20` (0x51aa20): hooks Three and Four, no depth test.
    PairThreeFour,
    /// `E43PairWalk::WA70` (0x51aa70): hooks Five and Six, no depth test.
    PairFiveSix,
    /// `E43PairWalk::WFE0` (0x51afe0): hook One, receiver deeper than `o2`.
    PairOneTwoDeeper,
    /// `E43PairWalk::WB050` (0x51b050): hook Two, receiver shallower than `o2`.
    PairOneTwoShallower,
    /// `E43PairWalk::WB0C0` (0x51b0c0): hooks Three and Four, receiver deeper than `o2`.
    PairThreeFourDeeper,
    /// `E43PairWalk::WB130` (0x51b130): hook Four, receiver shallower than `o2`.
    PairThreeFourShallower,
    /// `E43PairWalk::WB1A0` (0x51b1a0): hook Five, receiver deeper than `o2`.
    PairFiveSixDeeper,
    /// `E43PairWalk::WB210` (0x51b210): hook Six, receiver shallower than `o2`.
    PairFiveSixShallower,
    /// `E43PairWalk::WB320` (0x51b320): routes to `PairOneTwoDeeper`, `PairOneTwoShallower` or `PairOneTwo`.
    RoutePairOneTwo,
    /// `E43PairWalk::WB370` (0x51b370): routes to `PairThreeFourDeeper`, `PairThreeFourShallower` or
    /// `PairThreeFour`.
    RoutePairThreeFour,
    /// `E43PairWalk::WB3C0` (0x51b3c0): routes to `PairFiveSixDeeper`, `PairFiveSixShallower` or `PairFiveSix`.
    RoutePairFiveSix,
    /// `E43PairWalk::WAC0` (0x51aac0): the grid hooks (`E37Walk5E40`, `E37Walk5EB0`), no depth test.
    PairGrids,
    /// `E43PairWalk::WB280` (0x51b280): `E37Walk5E40`, receiver deeper than `o2`.
    PairGridsDeeper,
    /// `E43PairWalk::WB2D0` (0x51b2d0): `E37Walk5EB0`, receiver shallower than `o2`.
    PairGridsShallower,
    /// `E43PairWalk::WB410` (0x51b410): routes to `PairGridsDeeper`, `PairGridsShallower` or `PairGrids`.
    RoutePairGrids,
}

impl Walker {
    pub fn address(self) -> u32 {
        match self {
            Walker::PairOneTwo => 0x51A9D0,
            Walker::PairThreeFour => 0x51AA20,
            Walker::PairFiveSix => 0x51AA70,
            Walker::PairOneTwoDeeper => 0x51AFE0,
            Walker::PairOneTwoShallower => 0x51B050,
            Walker::PairThreeFourDeeper => 0x51B0C0,
            Walker::PairThreeFourShallower => 0x51B130,
            Walker::PairFiveSixDeeper => 0x51B1A0,
            Walker::PairFiveSixShallower => 0x51B210,
            Walker::RoutePairOneTwo => 0x51B320,
            Walker::RoutePairThreeFour => 0x51B370,
            Walker::RoutePairFiveSix => 0x51B3C0,
            Walker::PairGrids => 0x51AAC0,
            Walker::PairGridsDeeper => 0x51B280,
            Walker::PairGridsShallower => 0x51B2D0,
            Walker::RoutePairGrids => 0x51B410,
        }
    }
}

/// What a walker needs from the game: node reads, the hook calls and the child walks (each returns eax).
pub trait Walk {
    fn node(&self, at: u32) -> Node;
    /// `hook` of the node at `at`, called with (a, b).
    fn hook(&mut self, at: u32, hook: Hook, a: u32, b: u32);
    /// `which` called on `receiver` with (o2, a, b).
    fn walk(&mut self, which: Walker, receiver: u32, o2: u32, a: u32, b: u32) -> u32;
    /// The 16 bytes at `from` stored at `to` (`E43Mat16` copies).
    fn copy16(&mut self, to: u32, from: u32);
    /// The edge count of the contact grid at `grid` (`CColGrid`, its first pass's count), for the grid hooks.
    fn grid_count(&self, grid: u32) -> u32;
}

/// What a family calls on a node: one frame hook with `(a, b)`, or the grid walk of two hooks (`E37Walk5E40` and
/// `E37Walk5EB0`, see `grid_hooks`).
#[derive(Clone, Copy)]
enum Hooks {
    Frame(Hook),
    Grids(Hook, Hook),
}

/// A family of walkers: the hooks on the receiver (`lo`) and on `o2` (`hi`), and its equal-depth, deeper and
/// shallower walkers. `calls_equal`: the deeper and shallower walkers call the equal walker for the other depth
/// relations (else they run it inline).
struct Family {
    lo: Hooks,
    hi: Hooks,
    calls_equal: bool,
    equal: Walker,
    deeper: Walker,
    shallower: Walker,
}

const ONE_TWO: Family = Family {
    lo: Hooks::Frame(Hook::One),
    hi: Hooks::Frame(Hook::Two),
    calls_equal: false,
    equal: Walker::PairOneTwo,
    deeper: Walker::PairOneTwoDeeper,
    shallower: Walker::PairOneTwoShallower,
};

const THREE_FOUR: Family = Family {
    lo: Hooks::Frame(Hook::Three),
    hi: Hooks::Frame(Hook::Four),
    calls_equal: false,
    equal: Walker::PairThreeFour,
    deeper: Walker::PairThreeFourDeeper,
    shallower: Walker::PairThreeFourShallower,
};

const FIVE_SIX: Family = Family {
    lo: Hooks::Frame(Hook::Five),
    hi: Hooks::Frame(Hook::Six),
    calls_equal: false,
    equal: Walker::PairFiveSix,
    deeper: Walker::PairFiveSixDeeper,
    shallower: Walker::PairFiveSixShallower,
};

const GRIDS: Family = Family {
    lo: Hooks::Grids(Hook::One, Hook::Three),
    hi: Hooks::Grids(Hook::Two, Hook::Four),
    calls_equal: true,
    equal: Walker::PairGrids,
    deeper: Walker::PairGridsDeeper,
    shallower: Walker::PairGridsShallower,
};

/// The contact grids' stride (`CColGrid`, 0x6b0 bytes) and the two box tables the grid hooks get: the first pass's
/// (+0x40) and the second's (+0x220).
const GRID_STRIDE: u32 = 0x6B0;
const GRID_FIRST_BOXES: u32 = 0x40;
const GRID_SECOND_BOXES: u32 = 0x220;

/// `E37Walk5E40` (0x4f5e40, hooks One and Three) and `E37Walk5EB0` (0x4f5eb0, hooks Two and Four), stdcall
/// `(node, grids, n)`: for each of the `n` grids from `grids`, `first` on the node with the grid's first box table and
/// `second` with its second, each given the first grid's edge count (read again before each call). Returns `grids`.
pub fn grid_hooks<W: Walk>(w: &mut W, node: u32, grids: u32, n: u32, first: Hook, second: Hook) -> u32 {
    for i in 0..n {
        let grid = grids + GRID_STRIDE * i;
        let count = w.grid_count(grids);
        w.hook(node, first, grid + GRID_FIRST_BOXES, count);
        let count = w.grid_count(grids);
        w.hook(node, second, grid + GRID_SECOND_BOXES, count);
    }
    grids
}

fn hooks<W: Walk>(w: &mut W, hooks: Hooks, node: u32, a: u32, b: u32) {
    match hooks {
        Hooks::Frame(hook) => w.hook(node, hook, a, b),
        Hooks::Grids(first, second) => {
            grid_hooks(w, node, a, b, first, second);
        }
    }
}

/// Runs a walker. `receiver` is `this`, `o2` the other node.
pub fn run<W: Walk>(w: &mut W, which: Walker, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
    match which {
        Walker::PairOneTwo => pair(w, &ONE_TWO, receiver, o2, a, b),
        Walker::PairThreeFour => pair(w, &THREE_FOUR, receiver, o2, a, b),
        Walker::PairFiveSix => pair(w, &FIVE_SIX, receiver, o2, a, b),
        Walker::PairOneTwoDeeper => deeper(w, &ONE_TWO, receiver, o2, a, b),
        Walker::PairThreeFourDeeper => deeper(w, &THREE_FOUR, receiver, o2, a, b),
        Walker::PairFiveSixDeeper => deeper(w, &FIVE_SIX, receiver, o2, a, b),
        Walker::PairOneTwoShallower => shallower(w, &ONE_TWO, receiver, o2, a, b),
        Walker::PairThreeFourShallower => shallower(w, &THREE_FOUR, receiver, o2, a, b),
        Walker::PairFiveSixShallower => shallower(w, &FIVE_SIX, receiver, o2, a, b),
        Walker::RoutePairOneTwo => route(w, &ONE_TWO, receiver, o2, a, b),
        Walker::RoutePairThreeFour => route(w, &THREE_FOUR, receiver, o2, a, b),
        Walker::RoutePairFiveSix => route(w, &FIVE_SIX, receiver, o2, a, b),
        Walker::PairGrids => pair(w, &GRIDS, receiver, o2, a, b),
        Walker::PairGridsDeeper => deeper(w, &GRIDS, receiver, o2, a, b),
        Walker::PairGridsShallower => shallower(w, &GRIDS, receiver, o2, a, b),
        Walker::RoutePairGrids => route(w, &GRIDS, receiver, o2, a, b),
    }
}

/// `E43PairWalk::MB560` (0x51b560): copies the 16 bytes at `a` to `c`, then walks `receiver` against `b`
/// with `RoutePairThreeFour` (`c` as its `a`, 1 as its `b`).
pub fn mb560<W: Walk>(w: &mut W, receiver: u32, a: u32, b: u32, c: u32) {
    w.copy16(c, a);
    w.walk(Walker::RoutePairThreeFour, receiver, b, c, 1);
}

/// Equal-depth pair walk: the low hook on the receiver, the children walked by the family's equal walker, the high
/// hook on `o2`. Equal nodes end the walk.
fn pair<W: Walk>(w: &mut W, f: &Family, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
    if receiver != o2 {
        hooks(w, f.lo, receiver, a, b);
        let child = w.node(receiver).child;
        let o2_child = w.node(o2).child;
        w.walk(f.equal, child, o2_child, a, b);
        hooks(w, f.hi, o2, a, b);
    }
    a
}

/// The equal walker for a deeper or shallower one's other depth relations: a call, or inline.
fn equal<W: Walk>(w: &mut W, f: &Family, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
    if f.calls_equal {
        w.walk(f.equal, receiver, o2, a, b);
        return a;
    }
    pair(w, f, receiver, o2, a, b)
}

/// `WFE0`, `WB0C0`, `WB1A0` (the low hook; the receiver deeper than `o2` walks its own child against `o2`).
fn deeper<W: Walk>(w: &mut W, f: &Family, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
    if w.node(receiver).depth > w.node(o2).depth {
        hooks(w, f.lo, receiver, a, b);
        let child = w.node(receiver).child;
        w.walk(f.deeper, child, o2, a, b);
        return a;
    }
    equal(w, f, receiver, o2, a, b)
}

/// `WB050`, `WB130`, `WB210` (the high hook; the receiver shallower than `o2` walks itself against `o2`'s child,
/// with b = 1).
fn shallower<W: Walk>(w: &mut W, f: &Family, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
    if w.node(receiver).depth < w.node(o2).depth {
        let o2_child = w.node(o2).child;
        w.walk(f.shallower, receiver, o2_child, a, 1);
        hooks(w, f.hi, o2, a, b);
        return a;
    }
    equal(w, f, receiver, o2, a, b)
}

/// The dispatchers (`WB320`, `WB370`, `WB3C0`): nothing when the nodes are the same, else the family's walker for
/// the depth relation of the receiver and `o2`.
fn route<W: Walk>(w: &mut W, f: &Family, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
    if receiver == o2 {
        return a;
    }
    let (mine, theirs) = (w.node(receiver).depth, w.node(o2).depth);
    let which = if mine > theirs {
        f.deeper
    } else if mine < theirs {
        f.shallower
    } else {
        f.equal
    };
    w.walk(which, receiver, o2, a, b);
    a
}

/// What `lookup_factory_name` needs from the game: the factory table (`0x7c8040`) and the token cursor.
pub trait FactoryTable {
    /// The table's count (`CEntityFactory::Vf0C`, 0x502050, called on the table).
    fn count(&mut self) -> u32;
    /// The index of the name at `name` in the table (`NTweak::CDictBase::Find`, 0x52ebf0), when it is there.
    fn find(&mut self, name: u32) -> Option<u32>;
    /// Advances the token cursor at `cursor` (`NTweak::CTokenCursor::AdvanceIfSet`, 0x52ee50).
    fn advance(&mut self, cursor: u32);
    /// Appends `index` to the list of the walker `owner` (its count at +0x18, its indices from +0x1c).
    fn append_index(&mut self, owner: u32, index: u32);
}

/// `E43PairWalk::MF8B0` (0x5198b0): counts the table (the count loop), looks the name at `name` up, appends its
/// index to `owner` when the name is there, and advances the cursor.
pub fn lookup_factory_name<T: FactoryTable>(t: &mut T, owner: u32, name: u32, cursor: u32) {
    let mut n = 0;
    let mut count = t.count();
    while n < count {
        n += 1;
        count = t.count();
    }
    if let Some(index) = t.find(name) {
        t.append_index(owner, index);
    }
    t.advance(cursor);
}
