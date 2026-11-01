// The motion of what moves of itself on the screen (`renderer/motion.rs`, ours), for the temporal anti-aliasing
// (`taa.wgsl`): the meshes that move drawn again where the frame's depth is theirs (`Equal`), each pixel writing where
// its point was in the previous frame less where it is (uv, y down), and z 1, or 2 on the car's meshes. The car's parts
// kept in its model space (instance bit 0, `placed`) were where its previous body placed them; the meshes given in
// stage space where their previous positions (`previous`) were. Instance bit 1 marks the car's meshes.
struct Motion {
    // The current and the previous frame's unjittered view-projections, and the car's previous body.
    view_projection: mat4x4<f32>,
    previous_view_projection: mat4x4<f32>,
    previous_body: mat4x4<f32>,
};

@group(1) @binding(7) var<uniform> motion: Motion;

struct MotionOut {
    // As `vs_main`'s, so that the depth is the frame's to the bit.
    @invariant @builtin(position) clip: vec4<f32>,
    @location(0) now: vec4<f32>,
    @location(1) then: vec4<f32>,
    @location(2) @interpolate(flat) car: f32,
};

@vertex
fn vs_motion(input: VertexIn, @location(11) previous: vec3<f32>) -> MotionOut {
    var v = input;
    v.instance = input.instance & 1u;
    let p = placed(v).position;
    let was = select(previous, (motion.previous_body * vec4<f32>(input.position, 1.0)).xyz, v.instance == 1u);
    var out: MotionOut;
    out.clip = frame.view_projection * vec4<f32>(p, 1.0);
    out.now = motion.view_projection * vec4<f32>(p, 1.0);
    out.then = motion.previous_view_projection * vec4<f32>(was, 1.0);
    out.car = f32(input.instance >> 1u);
    return out;
}

@fragment
fn fs_motion(in: MotionOut) -> @location(0) vec4<f32> {
    // Behind the eye then: far off the screen, so the resolve takes the frame new.
    if (in.then.w <= 1.0e-6) {
        return vec4<f32>(4.0, 4.0, 1.0 + in.car, 0.0);
    }
    let moved = in.then.xy / in.then.w - in.now.xy / in.now.w;
    return vec4<f32>(moved * vec2<f32>(0.5, -0.5), 1.0 + in.car, 0.0);
}
