//! The field tables' Mem side: the value kinds, [`Field`], [`Object`] and `object!`, and the car's tabled objects.

use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;
use std::marker::PhantomData;

/// A value an object field holds: read and written as the game stores it (consecutive little-endian words or bytes).
pub trait Value: Copy {
    const LEN: u32;
    fn load(mem: &Mem, at: u32) -> Self;
    fn store(self, mem: &mut Mem, at: u32);
}

impl Value for u32 {
    const LEN: u32 = 4;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        mem.u32(at)
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        mem.set_u32(at, self)
    }
}

impl Value for i32 {
    const LEN: u32 = 4;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        mem.u32(at) as i32
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        mem.set_u32(at, self as u32)
    }
}

/// A float field. `f32::from_bits`/`to_bits` keep every bit pattern (NaN payloads included), so `lower` is exact; a
/// word the game puns (float bits in an int) is tabled as `u32` all the same.
impl Value for f32 {
    const LEN: u32 = 4;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        mem.f32(at)
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        mem.set_f32(at, self)
    }
}

impl Value for u8 {
    const LEN: u32 = 1;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        mem.u8(at)
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        mem.set_u8(at, self)
    }
}

impl Value for Vec3 {
    const LEN: u32 = 12;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        Vec3::load(mem, at)
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        Vec3::store(self, mem, at)
    }
}

impl Value for Vec4 {
    const LEN: u32 = 16;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        Vec4::load(mem, at)
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        Vec4::store(self, mem, at)
    }
}

impl<T: Value, const N: usize> Value for [T; N] {
    const LEN: u32 = T::LEN * N as u32;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        std::array::from_fn(|k| T::load(mem, at + T::LEN * k as u32))
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        for (k, v) in self.into_iter().enumerate() {
            v.store(mem, at + T::LEN * k as u32);
        }
    }
}

/// An `NTweak::CReal` body as objects embed it: the label (vtable) word, the name, the dirty word and the value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Real {
    pub label: u32,
    pub name: u32,
    pub dirty: u32,
    pub value: f32,
}

impl Value for Real {
    const LEN: u32 = 16;
    #[inline]
    fn load(mem: &Mem, at: u32) -> Self {
        Real { label: mem.u32(at), name: mem.u32(at + 4), dirty: mem.u32(at + 8), value: mem.f32(at + 12) }
    }
    #[inline]
    fn store(self, mem: &mut Mem, at: u32) {
        mem.set_u32(at, self.label);
        mem.set_u32(at + 4, self.name);
        mem.set_u32(at + 8, self.dirty);
        mem.set_f32(at + 12, self.value);
    }
}

/// A field of an object at offset `at`: its Mem accessor.
pub struct Field<T> {
    pub at: u32,
    value: PhantomData<T>,
}

impl<T> Clone for Field<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Field<T> {}

impl<T: Value> Field<T> {
    pub const fn new(at: u32) -> Self {
        Self { at, value: PhantomData }
    }

    /// The field of the object at `base`.
    #[inline]
    pub fn load(self, mem: &Mem, base: u32) -> T {
        T::load(mem, base + self.at)
    }

    #[inline]
    pub fn store(self, mem: &mut Mem, base: u32, value: T) {
        value.store(mem, base + self.at)
    }
}

/// One tabled field: its name, offset and length in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    pub name: &'static str,
    pub at: u32,
    pub len: u32,
}

/// An object with a field table (`object!`).
pub trait Object: Sized {
    const NAME: &'static str;
    const SIZE: u32;
    /// The fields in table order. Their bytes are the object's owned bytes.
    const FIELDS: &'static [FieldInfo];
    /// The object at `at`, every field read.
    fn lift(mem: &Mem, at: u32) -> Self;
    /// Writes every field of the object back at `at`.
    fn lower(&self, mem: &mut Mem, at: u32);

    /// The owned byte ranges of the object at `at` (`start..end`, one per field).
    fn owned(at: u32) -> impl Iterator<Item = std::ops::Range<u32>> {
        Self::FIELDS.iter().map(move |f| at + f.at..at + f.at + f.len)
    }

    /// The field that holds `offset`, if one does.
    fn field_at(offset: u32) -> Option<&'static FieldInfo> {
        Self::FIELDS.iter().find(|f| (f.at..f.at + f.len).contains(&offset))
    }
}

/// Tables an object's fields: `object! { /// doc  pub struct Name, size 0x30 { field @ 0x04: ty, ... } }`.
///
/// It generates the struct (one public field per row), a `Field` constant per row named as the field
/// (`Name::field.load(mem, at)`, `Name::field.at`) and the [`Object`] impl. The table must not overlap and must fit the
/// size; a test of each table checks that (`check_table`).
#[macro_export]
macro_rules! object {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident, size $size:literal {
            $( $(#[$fmeta:meta])* $field:ident @ $at:literal : $ty:ty ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        $vis struct $name {
            $( $(#[$fmeta])* pub $field: $ty, )*
        }

        #[allow(non_upper_case_globals, dead_code)]
        impl $name {
            $( pub const $field: $crate::store::Field<$ty> = $crate::store::Field::new($at); )*
        }

        impl $crate::store::Object for $name {
            const NAME: &'static str = stringify!($name);
            const SIZE: u32 = $size;
            const FIELDS: &'static [$crate::store::FieldInfo] = &[
                $( $crate::store::FieldInfo {
                    name: stringify!($field),
                    at: $at,
                    len: <$ty as $crate::store::Value>::LEN,
                }, )*
            ];
            fn lift(mem: &$crate::mem::Mem, at: u32) -> Self {
                Self { $( $field: <$ty as $crate::store::Value>::load(mem, at + $at), )* }
            }
            fn lower(&self, mem: &mut $crate::mem::Mem, at: u32) {
                $( <$ty as $crate::store::Value>::store(self.$field, mem, at + $at); )*
            }
        }
    };
}

/// Panics unless the fields of `T` are in offset order, do not overlap and fit `T::SIZE`.
pub fn check_table<T: Object>() {
    let mut end = 0;
    for f in T::FIELDS {
        assert!(f.at >= end, "{}::{} at {:#x} overlaps the field before (ends {end:#x})", T::NAME, f.name, f.at);
        end = f.at + f.len;
        assert!(end <= T::SIZE, "{}::{} ends at {end:#x}, past the size {:#x}", T::NAME, f.name, T::SIZE);
    }
}

/// The first owned byte of the object at `at` where `b` differs from `a`, as `(field, offset)`.
pub fn first_difference<T: Object>(a: &Mem, b: &Mem, at: u32) -> Option<(&'static str, u32)> {
    T::FIELDS.iter().find_map(|f| {
        let (x, y) = (a.read(at + f.at, f.len as usize), b.read(at + f.at, f.len as usize));
        x.iter().zip(&y).position(|(p, q)| p != q).map(|k| (f.name, f.at + k as u32))
    })
}

/// One tabled object of an image: its name, address and table.
pub struct Tabled {
    pub name: String,
    pub at: u32,
    pub fields: &'static [FieldInfo],
    lower_lift: fn(&Mem, &mut Mem, u32),
}

impl Tabled {
    pub fn of<T: Object>(name: impl Into<String>, at: u32) -> Self {
        Tabled { name: name.into(), at, fields: T::FIELDS, lower_lift: |mem, out, at| T::lift(mem, at).lower(out, at) }
    }

    /// `lower(lift(mem))` of the object, written into `out`.
    pub fn lower_lift(&self, mem: &Mem, out: &mut Mem) {
        (self.lower_lift)(mem, out, self.at)
    }

    /// The object's owned bytes, one range per field.
    pub fn owned(&self) -> impl Iterator<Item = (&'static FieldInfo, std::ops::Range<u32>)> + '_ {
        self.fields.iter().map(|f| (f, self.at + f.at..self.at + f.at + f.len))
    }

    /// The first owned byte where `b` differs from `a`, as `(field, offset in the object)`.
    pub fn first_difference(&self, a: &Mem, b: &Mem) -> Option<(&'static str, u32)> {
        self.owned().find_map(|(f, r)| {
            let len = (r.end - r.start) as usize;
            let (x, y) = (a.read(r.start, len), b.read(r.start, len));
            x.iter().zip(&y).position(|(p, q)| p != q).map(|k| (f.name, f.at + k as u32))
        })
    }
}

/// The tabled objects of the car at `car` (`object!` tables so far: the tyre scratches, the struts and their states,
/// the VCU scratch and its state).
pub fn car_objects(mem: &Mem, car: u32) -> Vec<Tabled> {
    use crate::{car_body, spring_damper, tyre, vcu, wheel};
    let mut out = Vec::new();
    for (k, wheel) in car_body::store::CarStore::wheel_pointers(mem, car).into_iter().enumerate() {
        if wheel == 0 {
            continue;
        }
        out.push(Tabled::of::<tyre::layout::ScratchObj>(
            format!("wheel{k} tyre scratch"),
            wheel + wheel::layout::TYRE_SCRATCH,
        ));
        let strut = wheel + wheel::layout::STRUT;
        out.push(Tabled::of::<spring_damper::layout::ControllerObj>(format!("wheel{k} strut"), strut));
        let state = spring_damper::layout::ControllerObj::state.load(mem, strut);
        out.push(Tabled::of::<spring_damper::layout::StateObj>(format!("wheel{k} strut state"), state));
    }
    let scratch = car_body::layout::scratch_at(car);
    out.push(Tabled::of::<vcu::layout::ScratchObj>("vcu scratch", scratch));
    let state = vcu::layout::ScratchObj::state.load(mem, scratch);
    out.push(Tabled::of::<vcu::layout::StateObj>("vcu state", state));
    out
}

/// Checks `lower(lift(image)) == image` on every owned byte of `objects`; the owned byte count, or the first byte that
/// differs.
pub fn round_trip(mem: &Mem, objects: &[Tabled]) -> Result<usize, String> {
    let mut out = Mem::new();
    for object in objects {
        object.lower_lift(mem, &mut out);
    }
    let mut bytes = 0;
    for object in objects {
        if let Some((field, offset)) = object.first_difference(mem, &out) {
            return Err(format!(
                "{} at {:#010x}: {field} (+{offset:#x}) differs after lower(lift)",
                object.name, object.at
            ));
        }
        bytes += object.fields.iter().map(|f| f.len as usize).sum::<usize>();
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::object! {
        /// A test object with every value kind.
        struct Sample, size 0x40 {
            word @ 0x00: u32,
            signed @ 0x04: i32,
            float @ 0x08: f32,
            byte @ 0x0c: u8,
            vec @ 0x10: Vec4,
            pair @ 0x20: [f32; 2],
            tail @ 0x30: Vec3,
        }
    }

    #[test]
    fn table_is_ordered_and_fits() {
        check_table::<Sample>();
        assert_eq!(Sample::vec.at, 0x10);
        assert_eq!(Sample::FIELDS.iter().map(|f| f.len).sum::<u32>(), 4 + 4 + 4 + 1 + 16 + 8 + 12);
    }

    #[test]
    fn lower_of_lift_is_the_image() {
        let mut mem = Mem::new();
        // NaN payloads and signalling NaNs survive the float fields.
        let bytes: Vec<u8> = (0..0x40u32).map(|k| (k * 37 + 0x81) as u8).collect();
        mem.map(0x1000, &bytes);
        mem.set_u32(0x1008, 0x7fa0_0001);
        mem.set_u32(0x1020, 0xffc0_1234);
        let image = mem.read(0x1000, 0x40);
        let object = Sample::lift(&mem, 0x1000);
        let mut out = Mem::new();
        out.map(0x1000, &[0; 0x40]);
        object.lower(&mut out, 0x1000);
        for range in Sample::owned(0x1000) {
            let len = (range.end - range.start) as usize;
            assert_eq!(out.read(range.start, len), mem.read(range.start, len), "{range:x?}");
        }
        assert_eq!(first_difference::<Sample>(&mem, &out, 0x1000), None);
        assert_eq!(Sample::float.load(&mem, 0x1000).to_bits(), 0x7fa0_0001);
        assert_eq!(image, mem.read(0x1000, 0x40));
        out.set_u8(0x1031, 0);
        assert_eq!(first_difference::<Sample>(&mem, &out, 0x1000), Some(("tail", 0x31)));
        assert_eq!(Sample::field_at(0x0d), None);
        assert_eq!(Sample::field_at(0x24).map(|f| f.name), Some("pair"));
    }
}
