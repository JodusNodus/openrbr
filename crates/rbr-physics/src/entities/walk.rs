//! The pair walkers over the entity frame trees. A walker takes a node (`receiver`) and another node `o2` of a second tree,
//! and walks the two in step: it calls a frame hook ([`Hook`]) on each node and walks their children. Which hooks and which
//! child walker depend on whether the receiver is deeper or shallower than `o2` (their depths). Every walker returns its `a`.
//!
//! The port calls hooks and child walks through `Walk`; the caller supplies the node reads and the calls.

use super::frame::Hook;
use super::layout::{GRID_FIRST_BOXES, GRID_SECOND_BOXES, GRID_STRIDE};

/// Four walkers that do nothing: they return the first argument; the receiver and the second argument are unused.
pub fn return_first(_this: u32, a: u32, _b: u32) -> u32 {
    a
}

/// A node of a walked tree: its child link and its depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub child: u32,
    pub depth: u32,
}

/// The walkers. Each name gives the hooks it calls and the depth relation it handles (`layout::walker_address` gives
/// the original's function).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walker {
    /// hooks One and Two, no depth test.
    PairOneTwo,
    /// hooks Three and Four, no depth test.
    PairThreeFour,
    /// hooks Five and Six, no depth test.
    PairFiveSix,
    /// hook One, receiver deeper than `o2`.
    PairOneTwoDeeper,
    /// hook Two, receiver shallower than `o2`.
    PairOneTwoShallower,
    /// hooks Three and Four, receiver deeper than `o2`.
    PairThreeFourDeeper,
    /// hook Four, receiver shallower than `o2`.
    PairThreeFourShallower,
    /// hook Five, receiver deeper than `o2`.
    PairFiveSixDeeper,
    /// hook Six, receiver shallower than `o2`.
    PairFiveSixShallower,
    /// routes to `PairOneTwoDeeper`, `PairOneTwoShallower` or `PairOneTwo`.
    RoutePairOneTwo,
    /// routes to `PairThreeFourDeeper`, `PairThreeFourShallower` or
    /// `PairThreeFour`.
    RoutePairThreeFour,
    /// routes to `PairFiveSixDeeper`, `PairFiveSixShallower` or `PairFiveSix`.
    RoutePairFiveSix,
    /// the grid hooks, no depth test.
    PairGrids,
    /// the first grid hook, receiver deeper than `o2`.
    PairGridsDeeper,
    /// the second grid hook, receiver shallower than `o2`.
    PairGridsShallower,
    /// routes to `PairGridsDeeper`, `PairGridsShallower` or `PairGrids`.
    RoutePairGrids,
}

/// What a walker needs from the game: node reads, the hook calls and the child walks
pub trait Walk {
    fn node(&self, at: u32) -> Node;
    /// `hook` of the node at `at`, called with (a, b).
    fn hook(&mut self, at: u32, hook: Hook, a: u32, b: u32);
    /// `which` called on `receiver` with (o2, a, b).
    fn walk(&mut self, which: Walker, receiver: u32, o2: u32, a: u32, b: u32) -> u32;
    /// The 16 bytes at `from` stored at `to`.
    fn copy16(&mut self, to: u32, from: u32);
    /// The edge count of the contact grid at `grid` (its first pass's count), for the grid hooks.
    fn grid_count(&self, grid: u32) -> u32;
}

/// What a family calls on a node: one frame hook with `(a, b)`, or the grid walk of two hooks (see `grid_hooks`).
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

/// The grid hooks (hooks One and Three, or Two and Four) on `(node, grids, n)`: for each of the `n` grids from `grids`, `first` on the node with the grid's first box table and
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

/// Copies the 16 bytes at `a` to `c`, then walks `receiver` against `b` with `RoutePairThreeFour` (`c` as its `a`, 1 as its
/// `b`): the vector at `a` carried from `receiver`'s frame to `b`'s, into `c`.
pub fn carry_three_four<W: Walk>(w: &mut W, receiver: u32, a: u32, b: u32, c: u32) {
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

/// The deeper walkers (the low hook; the receiver deeper than `o2` walks its own child against `o2`).
fn deeper<W: Walk>(w: &mut W, f: &Family, receiver: u32, o2: u32, a: u32, b: u32) -> u32 {
    if w.node(receiver).depth > w.node(o2).depth {
        hooks(w, f.lo, receiver, a, b);
        let child = w.node(receiver).child;
        w.walk(f.deeper, child, o2, a, b);
        return a;
    }
    equal(w, f, receiver, o2, a, b)
}

/// The shallower walkers (the high hook; the receiver shallower than `o2` walks itself against `o2`'s child,
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

/// The dispatchers: nothing when the nodes are the same, else the family's walker for
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

/// What `lookup_factory_name` needs from the game: the factory table and the token cursor.
pub trait FactoryTable {
    /// The table's count.
    fn count(&mut self) -> u32;
    /// The index of the name at `name` in the table, when it is there.
    fn find(&mut self, name: u32) -> Option<u32>;
    /// Advances the token cursor at `cursor` if it is set.
    fn advance(&mut self, cursor: u32);
    /// Appends `index` to the list of the walker `owner`.
    fn append_index(&mut self, owner: u32, index: u32);
}

/// Counts the table (the original loops over the count), looks the name at `name` up, appends its
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
