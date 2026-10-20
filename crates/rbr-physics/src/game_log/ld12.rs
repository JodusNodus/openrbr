//! msvcr71's `$I10_OUTPUT` (0x7c371e14 in the DLL as the game loads it), the digits `printf` gives a double: the value
//! as a 12-byte `_LDBL12` (a 16-bit extension word, the 64-bit mantissa, the exponent and sign), scaled by the CRT's
//! powers of ten (`__multtenpow12` 0x7c37331e, `___ld12mul` 0x7c3730ed) into `[0.1, 1)`, then 18 digits from its
//! 96-bit fixed point, the 18th rounding the 17th half up.

/// An `_LDBL12`, little-endian: the extension word at 0, the mantissa at 2 (64 bits), the exponent and sign at 10. As
/// the multiplication's 96-bit accumulator the same bytes are one integer.
#[derive(Clone, Copy)]
struct Ld12([u8; 12]);

impl Ld12 {
    const fn new(xt: u16, lo: u32, hi: u32, exp: u16) -> Self {
        let (x, l, h, e) = (xt.to_le_bytes(), lo.to_le_bytes(), hi.to_le_bytes(), exp.to_le_bytes());
        Ld12([x[0], x[1], l[0], l[1], l[2], l[3], h[0], h[1], h[2], h[3], e[0], e[1]])
    }

    fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.0[at], self.0[at + 1]])
    }

    fn set_u16(&mut self, at: usize, v: u16) {
        self.0[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn u32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.0[at..at + 4].try_into().unwrap())
    }

    fn set_u32(&mut self, at: usize, v: u32) {
        self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn int(&self) -> u128 {
        let mut b = [0; 16];
        b[..12].copy_from_slice(&self.0);
        u128::from_le_bytes(b)
    }

    fn set_int(&mut self, v: u128) {
        self.0.copy_from_slice(&v.to_le_bytes()[..12]);
    }

    /// `__shl_12` (0x7c372fb4), `__shr_12` (0x7c372fe2) and `__add_12` (0x7c372f56): the bytes as a 96-bit integer.
    fn shl(&mut self) {
        self.set_int(self.int() << 1);
    }

    fn shr(&mut self) {
        self.set_int(self.int() >> 1);
    }

    fn add(&mut self, other: &Ld12) {
        self.set_int(self.int() + other.int());
    }

    fn infinity(sign: u16) -> Self {
        Ld12::new(0, 0, 0x8000_0000, 0x7fff | sign)
    }
}

/// The CRT's `_pow10pos` and `_pow10neg` (0x7c38f690, 0x7c38f7f0), the first three groups, all a double needs:
/// `10^(j * 8^k)` for `k` 0 to 2 and `j` 1 to 7, the 80-bit mantissa rounded to nearest, and the 64 bits above the
/// extension word rounded up when the word is at least 0x8000 (the multiplication takes one off again).
const POW10_POS: [Ld12; 21] = [
    Ld12::new(0x0000, 0x00000000, 0xa0000000, 0x4002), // 1e1
    Ld12::new(0x0000, 0x00000000, 0xc8000000, 0x4005), // 1e2
    Ld12::new(0x0000, 0x00000000, 0xfa000000, 0x4008), // 1e3
    Ld12::new(0x0000, 0x00000000, 0x9c400000, 0x400c), // 1e4
    Ld12::new(0x0000, 0x00000000, 0xc3500000, 0x400f), // 1e5
    Ld12::new(0x0000, 0x00000000, 0xf4240000, 0x4012), // 1e6
    Ld12::new(0x0000, 0x00000000, 0x98968000, 0x4016), // 1e7
    Ld12::new(0x0000, 0x00000000, 0xbebc2000, 0x4019), // 1e8
    Ld12::new(0x0000, 0x04000000, 0x8e1bc9bf, 0x4034), // 1e16
    Ld12::new(0x0000, 0xcceda100, 0xd3c21bce, 0x404e), // 1e24
    Ld12::new(0xf020, 0x2b70b59e, 0x9dc5ada8, 0x4069), // 1e32
    Ld12::new(0x5dd0, 0x1ae525fd, 0xeb194f8e, 0x4083), // 1e40
    Ld12::new(0x9671, 0x0e4395d7, 0xaf298d05, 0x409e), // 1e48
    Ld12::new(0xbff9, 0x81ed44a0, 0x82818f12, 0x40b9), // 1e56
    Ld12::new(0x3cbf, 0xffcfa6d5, 0xc2781f49, 0x40d3), // 1e64
    Ld12::new(0xc66f, 0x80e98ce0, 0x93ba47c9, 0x41a8), // 1e128
    Ld12::new(0x85bc, 0x3927556b, 0xe070f78d, 0x427c), // 1e192
    Ld12::new(0xddbc, 0x9df9de8e, 0xaa7eebfb, 0x4351), // 1e256
    Ld12::new(0xe6a1, 0xf2cce376, 0x81842f29, 0x4426), // 1e320
    Ld12::new(0x1028, 0xaef8aa17, 0xc4c5e310, 0x44fa), // 1e384
    Ld12::new(0xa7eb, 0xebf7f3d4, 0x957a4ae1, 0x45cf), // 1e448
];
const POW10_NEG: [Ld12; 21] = [
    Ld12::new(0xcccd, 0xcccccccd, 0xcccccccc, 0x3ffb), // 1e-1
    Ld12::new(0x3d71, 0x70a3d70a, 0xa3d70a3d, 0x3ff8), // 1e-2
    Ld12::new(0x645a, 0x8d4fdf3b, 0x83126e97, 0x3ff5), // 1e-3
    Ld12::new(0xd3c3, 0xe219652c, 0xd1b71758, 0x3ff1), // 1e-4
    Ld12::new(0x0fd0, 0x1b478423, 0xa7c5ac47, 0x3fee), // 1e-5
    Ld12::new(0xa640, 0xaf6c69b6, 0x8637bd05, 0x3feb), // 1e-6
    Ld12::new(0x3d33, 0xe57a42bc, 0xd6bf94d5, 0x3fe7), // 1e-7
    Ld12::new(0xfdc2, 0x8461cefd, 0xabcc7711, 0x3fe4), // 1e-8
    Ld12::new(0x4c2f, 0xc44de15b, 0xe69594be, 0x3fc9), // 1e-16
    Ld12::new(0xc492, 0x44753b53, 0x9abe14cd, 0x3faf), // 1e-24
    Ld12::new(0x67de, 0x453994ba, 0xcfb11ead, 0x3f94), // 1e-32
    Ld12::new(0x2324, 0xbabce2c6, 0x8b61313b, 0x3f7a), // 1e-40
    Ld12::new(0x5561, 0xb17ec159, 0xbb127c53, 0x3f5f), // 1e-48
    Ld12::new(0xeed7, 0xbe068d2f, 0xfb158592, 0x3f44), // 1e-56
    Ld12::new(0x3f24, 0xa539e9a5, 0xa87fea27, 0x3f2a), // 1e-64
    Ld12::new(0xac7d, 0x64bce4a1, 0xddd0467c, 0x3e55), // 1e-128
    Ld12::new(0x7b63, 0x5423cc06, 0x91ff8377, 0x3d81), // 1e-192
    Ld12::new(0xfa91, 0x637a193a, 0xc0314325, 0x3cac), // 1e-256
    Ld12::new(0x8921, 0x478238d1, 0xfd00b897, 0x3bd7), // 1e-320
    Ld12::new(0x88dc, 0xb11b0858, 0xa686e3e8, 0x3b03), // 1e-384
    Ld12::new(0x84c6, 0xb6074245, 0xdb377599, 0x3a2e), // 1e-448
];

/// One tenth, the factor `$I10_OUTPUT` takes a scaled value of 1 or more below 1 with.
const TENTH: Ld12 = Ld12::new(0xcccc, 0xcccc_cccc, 0xcccc_cccc, 0x3ffb);

/// `___ld12mul(x, y)`: `x * y`, the 96-bit product of the two 80-bit mantissas (its partial products of 16-bit words,
/// a carry out of a word going into the word two above), normalised, rounded to nearest at the extension word's bit 15
/// (ties to even through bit 16), with the CRT's overflow to infinity and underflow to zero.
fn mul(x: &mut Ld12, y: &Ld12) {
    let sign = (x.u16(10) ^ y.u16(10)) & 0x8000;
    let (ex, ey) = (u32::from(x.u16(10) & 0x7fff), u32::from(y.u16(10) & 0x7fff));
    let mut sum = ex + ey;
    if ex >= 0x7fff || ey >= 0x7fff || sum as u16 > 0xbffd {
        *x = Ld12::infinity(sign);
        return;
    }
    if sum as u16 <= 0x3fbf {
        *x = Ld12([0; 12]);
        return;
    }
    let zero = |v: &Ld12| v.u32(8) & 0x7fff_ffff == 0 && v.u32(4) == 0 && v.u32(0) == 0;
    if ex == 0 {
        sum += 1;
        if zero(x) {
            x.set_u16(10, 0);
            return;
        }
    }
    if ey == 0 {
        sum += 1;
        if zero(y) {
            *x = Ld12([0; 12]);
            return;
        }
    }
    // The 12 bytes and the word above them, which a carry out of the top partial sum would reach.
    let mut t = [0u8; 14];
    for i in 0..5 {
        for j in 0..5 - i {
            let prod = u32::from(x.u16(2 * i + 2 * j)) * u32::from(y.u16(8 - 2 * j));
            let r = 2 * i;
            let (v, carry) = u32::from_le_bytes(t[r..r + 4].try_into().unwrap()).overflowing_add(prod);
            t[r..r + 4].copy_from_slice(&v.to_le_bytes());
            if carry {
                let w = u16::from_le_bytes([t[r + 4], t[r + 5]]).wrapping_add(1);
                t[r + 4..r + 6].copy_from_slice(&w.to_le_bytes());
            }
        }
    }
    let mut m = Ld12(t[..12].try_into().unwrap());
    sum = sum.wrapping_add(0xc002);
    while sum as u16 as i16 > 0 && m.0[11] & 0x80 == 0 {
        m.shl();
        sum = sum.wrapping_add(0xffff);
    }
    if sum as u16 as i16 <= 0 {
        sum = sum.wrapping_add(0xffff);
        if (sum as u16 as i16) < 0 {
            let n = sum.wrapping_neg() as u16;
            sum = sum.wrapping_add(u32::from(n));
            let mut sticky = false;
            for _ in 0..n {
                sticky |= m.0[0] & 1 != 0;
                m.shr();
            }
            if sticky {
                m.0[0] |= 1;
            }
        }
    }
    if m.u16(0) > 0x8000 || m.u32(0) & 0x1ffff == 0x18000 {
        if m.u32(2) == u32::MAX {
            m.set_u32(2, 0);
            if m.u32(6) == u32::MAX {
                m.set_u32(6, 0);
                if m.u16(10) == u16::MAX {
                    sum = sum.wrapping_add(1);
                    m.set_u16(10, 0x8000);
                } else {
                    m.set_u16(10, m.u16(10) + 1);
                }
            } else {
                m.set_u32(6, m.u32(6) + 1);
            }
        } else {
            m.set_u32(2, m.u32(2) + 1);
        }
    }
    if sum as u16 >= 0x7fff {
        *x = Ld12::infinity(sign);
    } else {
        *x = Ld12::new(m.u16(2), m.u32(4), m.u32(8), sum as u16 | sign);
    }
}

/// `__multtenpow12(x, pow, 1)`: `x` times `10^pow`, one table factor per octal digit of `|pow|`, each with its
/// rounded-up word taken back (`|pow|` below 512).
fn mul_pow10(x: &mut Ld12, pow: i32) {
    let table = if pow < 0 { &POW10_NEG } else { &POW10_POS };
    let mut pow = pow.unsigned_abs();
    let mut group = 0;
    while pow != 0 {
        let last = (pow & 7) as usize;
        pow >>= 3;
        if last != 0 {
            let mut y = table[7 * group + last - 1];
            if y.u16(0) >= 0x8000 {
                y.set_u32(2, y.u32(2).wrapping_sub(1));
            }
            mul(x, &y);
        }
        group += 1;
    }
}

/// `$I10_OUTPUT(ld, 17, 0)` on a finite nonzero double `v` (`__dtold` makes the long double): the digits without trailing
/// zeros and the decimal exponent (`v = 0.d1d2... * 10^exp`).
pub(super) fn digits(v: f64) -> (Vec<u8>, i32) {
    let bits = v.to_bits();
    let e = ((bits >> 52) & 0x7ff) as u32;
    let frac = bits & ((1 << 52) - 1);
    let (mut expn, mut man) = if e == 0 { (0x3c01, frac << 11) } else { (e + 0x3c00, (frac | 1 << 52) << 11) };
    while man >> 63 == 0 {
        man <<= 1;
        expn -= 1;
    }
    let (hi, lo) = ((man >> 32) as u32, man as u32);
    let est = ((expn >> 8) + 2 * (hi >> 24)) * 0x4d + expn * 0x4d10;
    let mut r10 = i32::from((est.wrapping_sub(0x1343_12f4) as i32 >> 16) as i16);
    let mut x = Ld12::new(0, lo, hi, expn as u16);
    mul_pow10(&mut x, -r10);
    if x.u16(10) >= 0x3fff {
        r10 += 1;
        mul(&mut x, &TENTH);
    }
    let shift = i32::from(x.u16(10)) - 0x3ffe;
    x.set_u16(10, 0);
    for _ in 0..8 {
        x.shl();
    }
    if shift < 0 {
        for _ in 0..(-shift) & 0xff {
            x.shr();
        }
    }
    let mut out = Vec::with_capacity(18);
    for _ in 0..18 {
        let tmp = x;
        x.shl();
        x.shl();
        x.add(&tmp);
        x.shl();
        out.push(b'0' + x.0[11]);
        x.0[11] = 0;
    }
    let last = out.pop().unwrap();
    if last >= b'5' {
        let mut k = out.len();
        while k > 0 && out[k - 1] == b'9' {
            out[k - 1] = b'0';
            k -= 1;
        }
        if k == 0 {
            k = 1;
            r10 += 1;
        }
        out[k - 1] += 1;
        out.truncate(k);
    } else {
        while out.last() == Some(&b'0') {
            out.pop();
        }
        if out.is_empty() {
            return (b"0".to_vec(), 0);
        }
    }
    (out, r10)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes at `va` of the DLL `file` as it loads (its sections by their virtual addresses).
    fn read(file: &[u8], va: u32, len: usize) -> &[u8] {
        let word = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
        let pe = word(0x3c) as usize;
        let sections = usize::from(u16::from_le_bytes([file[pe + 6], file[pe + 7]]));
        let optional = usize::from(u16::from_le_bytes([file[pe + 20], file[pe + 21]]));
        let base = word(pe + 24 + 28);
        (0..sections)
            .map(|i| pe + 24 + optional + 40 * i)
            .find_map(|s| {
                let (start, size, raw) = (base + word(s + 12), word(s + 8), word(s + 20) as usize);
                (start..start + size).contains(&va).then(|| &file[raw + (va - start) as usize..][..len])
            })
            .expect("an address in a section")
    }

    #[test]
    fn powers_of_ten_are_the_dlls() {
        let Ok(data) = std::env::var("RBR_DATA") else {
            eprintln!("skipping: RBR_DATA not set");
            return;
        };
        let file = std::fs::read(format!("{data}/Root/msvcr71.dll")).expect("Root/msvcr71.dll");
        for (table, at) in [(&POW10_POS, 0x7c38_f690), (&POW10_NEG, 0x7c38_f7f0)] {
            for (k, entry) in table.iter().enumerate() {
                assert_eq!(&entry.0[..], read(&file, at + 12 * k as u32, 12), "entry {k} at {at:#x}");
            }
        }
    }

    #[test]
    fn digits_are_seventeen_without_trailing_zeros() {
        assert_eq!(digits(1.5), (b"15".to_vec(), 1));
        assert_eq!(digits(0.1), (b"10000000000000001".to_vec(), 0));
        assert_eq!(digits(1e30), (b"1".to_vec(), 31));
    }
}
