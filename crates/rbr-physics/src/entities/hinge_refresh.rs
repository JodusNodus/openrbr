//! The hinged body's refresh: it copies the definition's angle block and limit states into the body, sets the hinge angle
//! from where the definition's default angle lies between its two limits, builds the definition's two frames
//! ([`super::hinge_axes`]), composes the pose with the car, poses the body ([`super::hinge_pose`]), finds its offset
//! ([`super::hinge_offset`]) and adds `vecHingePoint_BF` to the grid as an edge.
//!
//! The calls are the caller's ([`RefreshCalls`]). Each one can change the body (the pose, the wake-up), so the model
//! reads the body back after every call (`load`) and writes its own values before every call (`store`). The fourth lane of
//! the offset copy is not set by the original (its stack temporary is left as it was); the model writes 0. The angle is
//! stored as float bits: the original moves the float and does not convert it to an integer.

/// The body and definition words the model reads or writes, as bits (the copies are word copies).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefreshState {
    /// `Unlocked`: zero marks the angle block dirty and clears the dirty counter, nonzero wakes the body.
    pub unlocked: u32,
    /// The definition's angle block; its last word is `HingeDefaultAngle`.
    pub definition_angle_block: [u32; 3],
    /// `HingeMaxAngle` and `HingeMinAngle`: the default angle's fraction between them is
    /// `(default - min) / (max - min)`.
    pub max_angle: u32,
    pub min_angle: u32,
    /// The definition's limit springs' positions (`Spring0Pos`, `Spring1Pos`): the body's
    /// [`super::hinge_limits::HingeState::limit_state`] start from them.
    pub definition_limit_state: [u32; 2],
    /// `vecHingePoint_BF` (4 words): the grid edge's vector.
    pub hinge_point_bf: [u32; 4],
    /// Where the link flag is, and the flag. The model clears the flag when it wakes the body.
    pub link_address: u32,
    pub link_flag: u32,
    /// The grid, when nonzero: it takes `hinge_point_bf` as an edge.
    pub grid: u32,
    /// The dirty marker of the body's tweak object, which the dirty-marking sets to 1, and the counter beside it, which the
    /// zero-gate branch clears (named by use).
    pub dirty: u32,
    pub dirty_counter: u32,
    /// The angle block the body keeps: the definition's copy, with the middle word set to 1 (the dirty marker) and the last
    /// word (the hinge angle) from the default angle's fraction when the link flag is zero.
    pub angle_block: [u32; 3],
    /// The body's limit states.
    pub limit_state: [u32; 2],
    /// The body's position (4 words) and its copy.
    pub position: [u32; 4],
    pub position_copy: [u32; 4],
    /// The body's orientation quaternion (4 words) and its copy.
    pub orientation: [u32; 4],
    pub orientation_copy: [u32; 4],
    /// The body's offset (4 words) and its copy; and the copy of the offset result the offset call returns.
    pub offset: [u32; 4],
    pub offset_copy: [u32; 4],
    pub offset_result_copy: [u32; 4],
}

/// The object handles (the game's addresses, opaque to the model) the refresh passes to its calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshHandles {
    /// The body.
    pub body: u32,
    /// The body's car: the receiver of the pose composition.
    pub car: u32,
    /// The first frame build's six handles ([`super::hinge_axes::AxesHandles`]'s fields in order): from the `_RF` vectors.
    pub first_axes: [u32; 6],
    /// The second frame build's six handles: from the `_BF` vectors.
    pub second_axes: [u32; 6],
    /// The pose composition's input and its output.
    pub compose_input: u32,
    pub compose_output: u32,
    /// Where the offset call writes its result (the caller's scratch vector).
    pub offset_out: u32,
}

/// What the refresh needs from the game. Every call takes the body as it is in memory (the model stores first).
pub trait RefreshCalls {
    /// The body and definition words as they are now.
    fn load(&self) -> RefreshState;
    /// Writes the model's values back to the body (the words the model changes).
    fn store(&mut self, state: &RefreshState);
    /// Wakes the body (the plain solid's wake-up call).
    fn wake_body(&mut self, body: u32);
    /// Builds a frame on the body from its six handles ([`super::hinge_axes`]).
    fn build_axes(&mut self, body: u32, handles: [u32; 6]);
    /// Composes `input` with the car's pose into `output`.
    fn compose_with_car(&mut self, car: u32, input: u32, output: u32);
    /// Poses the body ([`super::hinge_pose::update_pose`]).
    fn update_pose(&mut self, body: u32);
    /// The body's offset ([`super::hinge_offset::hinge_offset`]), written to `out`; returns its three floats.
    fn offset(&mut self, body: u32, out: u32) -> [f32; 3];
    /// Adds `vector` to `grid` as an edge.
    fn add_grid_edge(&mut self, grid: u32, vector: [f32; 4]);
}

/// The weight of the upper limit when the default angle is past the middle between the limits, and the other way round below
/// the middle (0.9 and 0.1, f32); the lower weight is 1 minus it.
const NEAR_WEIGHT: f32 = 0.9;
const FAR_WEIGHT: f32 = 0.1;

/// Refreshes the hinged body, in the original's call order.
pub fn refresh_hinge<C: RefreshCalls>(calls: &mut C, handles: &RefreshHandles) {
    let mut s = calls.load();
    if s.unlocked == 0 {
        // the dirty marker is set, then the counter is cleared
        s.dirty = 1;
        s.dirty_counter = 0;
    } else {
        s.link_flag = 0;
        calls.store(&s);
        calls.wake_body(handles.body);
        s = calls.load();
    }
    s.angle_block = s.definition_angle_block;
    if s.link_flag == 0 {
        let max = f32::from_bits(s.max_angle);
        let min = f32::from_bits(s.min_angle);
        let default = f32::from_bits(s.definition_angle_block[2]);
        // the default angle's fraction between the two limits: past the middle, the weight is 0.9
        let high_weight = if 0.5 < (default - min) / (max - min) { NEAR_WEIGHT } else { FAR_WEIGHT };
        let low_weight = 1.0 - high_weight;
        // the angle block's dirty marker, then the angle: the weights summed, times the maximum angle
        s.angle_block[1] = 1;
        s.angle_block[2] = ((low_weight + high_weight) * max).to_bits();
    }
    calls.store(&s);
    calls.build_axes(handles.body, handles.first_axes);
    calls.build_axes(handles.body, handles.second_axes);
    s = calls.load();
    calls.store(&s);
    calls.compose_with_car(handles.car, handles.compose_input, handles.compose_output);
    s = calls.load();
    calls.store(&s);
    calls.update_pose(handles.body);
    s = calls.load();
    s.position_copy = s.position;
    s.orientation_copy = s.orientation;
    calls.store(&s);
    let offset = calls.offset(handles.body, handles.offset_out);
    s = calls.load();
    s.offset_copy = s.offset;
    s.offset_result_copy = [offset[0].to_bits(), offset[1].to_bits(), offset[2].to_bits(), 0];
    if s.grid != 0 {
        calls.store(&s);
        calls.add_grid_edge(s.grid, s.hinge_point_bf.map(f32::from_bits));
        s = calls.load();
    }
    s.limit_state = s.definition_limit_state;
    calls.store(&s);
}
