//! The driveline sampler: the car's position along the stage (`CCar` +0x17c) to the distance the stage markers are keyed
//! by (rbr-recomp `notes/formats/stage-markers.md`, "Units").

use rbr_formats::trk::DriveLineRecord;
use rbr_physics::math::rsqrt::rsqrt;

/// `E12_Sampler::Sample`'s second `.trk` vector's xyz lanes. Dot150 ignores the aligned fourth lane.
pub fn sample_direction(records: &[DriveLineRecord], position: f32) -> Option<[f32; 3]> {
    if records.len() < 2 {
        return None;
    }

    // The x86 path uses cvttss2si here. For the recorded, finite positions this is truncation toward zero,
    // including the short negative interval that still selects record zero.
    let index = position as i32;
    if index < 0 {
        return Some(direction(&records[0]));
    }
    let index = index as usize;
    if index >= records.len() - 1 {
        return Some(direction(records.last()?));
    }

    let fraction = position - index as f32;
    let from = direction(&records[index]);
    let to = direction(&records[index + 1]);
    Some(std::array::from_fn(|lane| {
        let delta = to[lane] - from[lane];
        let scaled = delta * fraction;
        scaled + from[lane]
    }))
}

fn direction(record: &DriveLineRecord) -> [f32; 3] {
    [record.vectors[1].x, record.vectors[1].y, record.vectors[1].z]
}

/// `E12_Sampler::Dot150` (0x476450): normalize the sampled TRK direction and dot it with the car's
/// velocity lanes at `CCar+0x1c0/+0x1c4/+0x1c8`.
pub fn dot150(records: &[DriveLineRecord], position: f32, velocity: [f32; 3]) -> Option<f32> {
    let mut sampled = sample_direction(records, position)?;
    let length_squared = (sampled[0] * sampled[0] + sampled[1] * sampled[1]) + sampled[2] * sampled[2];
    // E01Vec4::Normalize uses `rsqrtss`, the reference CPU's approximation, not a correctly rounded sqrt/divide.
    let inverse_length = rsqrt(length_squared);
    for lane in &mut sampled {
        *lane *= inverse_length;
    }

    // Dot150 returns through x87 as `(y + z) + x`. The game's x87 runs at 24-bit precision (Direct3D's default
    // control word), so every product and sum rounds to f32.
    Some((sampled[1] * velocity[1] + sampled[2] * velocity[2]) + sampled[0] * velocity[0])
}

/// `E12_SampleX` (0x475ea0): the `value` of the `.trk` kind-0x14 records at `floor(x)`, interpolated linearly to the
/// next record by the fraction of `x`. The last record is held beyond the end. `None` where the original logs an
/// out-of-range index (`x` negative or past the last record's index), which it returns unset.
pub fn sample_x(values: &[f32], x: f32) -> Option<f32> {
    let i = usize::try_from(x.floor() as i64).ok()?;
    let v = *values.get(i)?;
    let Some(&next) = values.get(i + 1) else {
        return Some(v);
    };
    let frac = x - i as f32;
    Some((next - v) * frac + v)
}

/// The car's speed above which `M579590` (0x579590, the HUD's `CSigCarGeneralState` receiver) can call it the wrong way,
/// m/s (`DAT_0072b2a0`).
const WRONG_WAY_SPEED: f32 = 3.0;

/// `M579590`'s wrong-way test: the velocity points back along the driveline's direction at the car (`CDriveLine::LerpAt`
/// 0x50a920 at HUD +0x13c, the car's driveline parameter), the car's `+Y` axis (its rear: the nose is `-Y`) points
/// forward along it, and the car is faster than 3 m/s. The direction need not be unit length; the velocity's is only
/// normalized above 0, so only the signs matter.
pub fn wrong_way(velocity: [f32; 3], rear: [f32; 3], direction: [f32; 3]) -> bool {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let speed = dot(velocity, velocity).sqrt();
    dot(velocity, direction) < 0.0 && dot(rear, direction) > 0.0 && speed > WRONG_WAY_SPEED
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: [f32; 3] = [0.0, 10.0, 30.0];

    #[test]
    fn the_wrong_way_is_backwards_along_the_line_nose_first() {
        let along = [0.0, 1.0, 0.0];
        // Nose (-Y) back along the line, moving back at 5 m/s.
        assert!(wrong_way([0.0, -5.0, 0.0], [0.0, 1.0, 0.0], along));
        assert!(!wrong_way([0.0, -2.0, 0.0], [0.0, 1.0, 0.0], along), "slower than 3 m/s");
        assert!(!wrong_way([0.0, 5.0, 0.0], [0.0, -1.0, 0.0], along), "the right way");
        assert!(!wrong_way([0.0, -5.0, 0.0], [0.0, -1.0, 0.0], along), "reversing back down the stage");
    }

    #[test]
    fn interpolates_between_records() {
        assert_eq!(sample_x(&VALUES, 0.0), Some(0.0));
        assert_eq!(sample_x(&VALUES, 0.5), Some(5.0));
        assert_eq!(sample_x(&VALUES, 1.25), Some(15.0));
    }

    #[test]
    fn holds_the_last_record() {
        assert_eq!(sample_x(&VALUES, 2.0), Some(30.0));
        assert_eq!(sample_x(&VALUES, 2.5), Some(30.0));
    }

    #[test]
    fn out_of_range_is_none() {
        assert_eq!(sample_x(&VALUES, 3.0), None);
        assert_eq!(sample_x(&VALUES, -0.5), None);
    }
}
