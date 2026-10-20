//! The game's logger: `E01Log(channel, level, format, ...)` (0x43c8e0), which hands its arguments to `GameLog`
//! (0x490f50). The retail game logs: the console flag (`0x7c18f8`) is clear, and the channel and level pick a sink from
//! the table at 0x7c29d8. Sink 2 (every channel 6 and 7 the physics logs on) writes the text with `vsprintf` into the
//! static buffer at 0x7ece58, copies it to a new block, hands that to `OutputDebugStringA` and frees it (0x4dedb0);
//! sink 1 writes to the log file when one is open (0x4ded80); sink 0 is none. [`format`] is msvcr71's `vsprintf` on
//! the conversions the exe's log formats use.

pub mod layout;
mod ld12;

pub use layout::{log, LogHost, BUFFER};

/// A log call the port does not cover; nothing is written when it says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported(pub &'static str);

/// What `vsprintf` reads for a conversion: the next argument word, and the string at an address.
pub trait Args {
    fn word(&mut self) -> u32;
    fn string(&self, at: u32) -> Vec<u8>;
}

/// msvcr71's `vsprintf(buf, fmt, args)` (`_output`): the text it writes, without its NUL. Covers the flags `-+ #0`,
/// width and precision (digits or `*`), the sizes `h` and `l`, and `d i u x X c s p f %`; anything else is
/// [`Unsupported`].
pub fn format(fmt: &[u8], args: &mut impl Args) -> Result<Vec<u8>, Unsupported> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        i += 1;
        if c != b'%' {
            out.push(c);
            continue;
        }
        let mut spec = Spec::default();
        while let Some(&f) = fmt.get(i).filter(|f| b"-+ #0".contains(f)) {
            match f {
                b'-' => spec.left = true,
                b'+' => spec.plus = true,
                b' ' => spec.space = true,
                b'#' => spec.alt = true,
                _ => spec.zero = true,
            }
            i += 1;
        }
        if fmt.get(i) == Some(&b'*') {
            let w = args.word() as i32;
            if w < 0 {
                spec.left = true;
            }
            spec.width = w.unsigned_abs() as usize;
            i += 1;
        } else {
            while let Some(d) = fmt.get(i).filter(|d| d.is_ascii_digit()) {
                spec.width = spec.width * 10 + usize::from(d - b'0');
                i += 1;
            }
        }
        if fmt.get(i) == Some(&b'.') {
            i += 1;
            let mut p = 0usize;
            if fmt.get(i) == Some(&b'*') {
                // A negative precision is taken as none.
                let v = args.word() as i32;
                spec.precision = (v >= 0).then_some(v as usize);
                i += 1;
            } else {
                while let Some(d) = fmt.get(i).filter(|d| d.is_ascii_digit()) {
                    p = p * 10 + usize::from(d - b'0');
                    i += 1;
                }
                spec.precision = Some(p);
            }
        }
        let mut short = false;
        while let Some(&s) = fmt.get(i).filter(|s| matches!(s, b'h' | b'l')) {
            short = s == b'h';
            i += 1;
        }
        let Some(&conv) = fmt.get(i) else { break };
        i += 1;
        let field = match conv {
            b'%' => {
                out.push(b'%');
                continue;
            }
            b'd' | b'i' => {
                let w = args.word();
                let v = if short { i64::from(w as i16) } else { i64::from(w as i32) };
                spec.integer(v.unsigned_abs(), v < 0, 10, false, b"")
            }
            b'u' | b'x' | b'X' => {
                let w = args.word();
                let v = u64::from(if short { w & 0xffff } else { w });
                let (base, prefix): (u64, &[u8]) = match conv {
                    b'u' => (10, b""),
                    b'x' => (16, b"0x"),
                    _ => (16, b"0X"),
                };
                let prefix = if spec.alt && v != 0 { prefix } else { b"" };
                spec.integer(v, false, base, conv == b'X', prefix)
            }
            b'p' => {
                let v = u64::from(args.word());
                spec.precision = Some(8);
                spec.integer(v, false, 16, true, b"")
            }
            b'c' => spec.pad(Vec::new(), vec![args.word() as u8]),
            b's' => {
                let at = args.word();
                let mut text = if at == 0 { b"(null)".to_vec() } else { args.string(at) };
                if let Some(p) = spec.precision {
                    text.truncate(p);
                }
                spec.pad(Vec::new(), text)
            }
            b'f' => {
                let lo = args.word();
                let hi = args.word();
                let v = f64::from_bits(u64::from(hi) << 32 | u64::from(lo));
                spec.fixed(v)
            }
            _ => return Err(Unsupported("a log conversion the port does not cover")),
        };
        out.extend(field);
    }
    Ok(out)
}

/// One conversion's flags, width and precision.
#[derive(Default)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
}

impl Spec {
    /// The sign or prefix `head`, then `body`, padded to the width: spaces on the left (or right with `-`), zeros
    /// between the two with `0`.
    fn pad(&self, head: Vec<u8>, body: Vec<u8>) -> Vec<u8> {
        let fill = self.width.saturating_sub(head.len() + body.len());
        let mut out = Vec::with_capacity(self.width.max(head.len() + body.len()));
        if !self.left && !self.zero {
            out.resize(fill, b' ');
        }
        out.extend(head);
        if !self.left && self.zero {
            out.resize(out.len() + fill, b'0');
        }
        out.extend(body);
        if self.left {
            out.resize(out.len() + fill, b' ');
        }
        out
    }

    fn sign(&self, negative: bool) -> Vec<u8> {
        match (negative, self.plus, self.space) {
            (true, _, _) => b"-".to_vec(),
            (false, true, _) => b"+".to_vec(),
            (false, false, true) => b" ".to_vec(),
            _ => Vec::new(),
        }
    }

    /// An integer: at least `precision` digits (a zero with precision 0 has none); a precision drops the `0` flag.
    fn integer(mut self, v: u64, negative: bool, base: u64, upper: bool, prefix: &[u8]) -> Vec<u8> {
        let digits: &[u8] = if upper { b"0123456789ABCDEF" } else { b"0123456789abcdef" };
        let mut body = Vec::new();
        let mut n = v;
        while n != 0 {
            body.push(digits[(n % base) as usize]);
            n /= base;
        }
        let min = self.precision.unwrap_or(1);
        while body.len() < min {
            body.push(b'0');
        }
        body.reverse();
        if self.precision.is_some() {
            self.zero = false;
        }
        let mut head = self.sign(negative);
        head.extend_from_slice(prefix);
        self.pad(head, body)
    }

    /// `%f`: `_cftof` on `_fltout2`'s digits ([`digits`]) rounded by `_fptostr` to the precision.
    fn fixed(&self, v: f64) -> Vec<u8> {
        let ndec = self.precision.unwrap_or(6);
        let (negative, mantissa, decpt) = digits(v);
        let (mut buf, decpt) = round_digits(&mantissa, decpt + ndec as i32, decpt);
        let mut body = Vec::new();
        if decpt <= 0 {
            body.push(b'0');
        } else {
            body.extend(buf.drain(..decpt as usize));
        }
        if ndec > 0 || self.alt {
            body.push(b'.');
        }
        if ndec > 0 {
            if decpt < 0 {
                let zeros = (-decpt as usize).min(ndec);
                body.resize(body.len() + zeros, b'0');
            }
            body.extend(buf);
        }
        self.pad(self.sign(negative), body)
    }
}

/// `_fltout2` (`$I10_OUTPUT` with 17 digits, [`ld12::digits`]): the sign, the significant digits without trailing zeros and the decimal
/// point's place (`decpt`: digits before it). Zero is `"0"` at 0, without a sign (`-0.0` too); the others are msvcr71's `1#INF`, `1#QNAN`, `1#SNAN`
/// and `1#IND` (the default NaN, sign set) at 1.
fn digits(v: f64) -> (bool, Vec<u8>, i32) {
    let negative = v.is_sign_negative();
    if v.is_infinite() {
        return (negative, b"1#INF".to_vec(), 1);
    }
    if v.is_nan() {
        let bits = v.to_bits();
        let quiet = bits & (1 << 51) != 0;
        let text: &[u8] = match (quiet, bits & ((1 << 51) - 1) == 0 && negative) {
            (true, true) => b"1#IND",
            (true, false) => b"1#QNAN",
            _ => b"1#SNAN",
        };
        return (negative, text.to_vec(), 1);
    }
    if v == 0.0 {
        return (false, b"0".to_vec(), 0);
    }
    let (mantissa, decpt) = ld12::digits(v);
    (negative, mantissa, decpt)
}

/// `_fptostr(buf, count, flt)`: the first `count` digits of `mantissa` (padded with zeros), rounded up on the next
/// digit when it is at least `'5'` (a character compare, so the letters of `#INF` round up too); a carry out of the
/// first digit moves the point. Returns the digits and the point.
fn round_digits(mantissa: &[u8], count: i32, decpt: i32) -> (Vec<u8>, i32) {
    let mut buf = vec![b'0'];
    let mut next = 0;
    for _ in 0..count.max(0) {
        buf.push(mantissa.get(next).copied().unwrap_or(b'0'));
        next += usize::from(next < mantissa.len());
    }
    if count >= 0 && mantissa.get(next).is_some_and(|&c| c >= b'5') {
        let mut k = buf.len() - 1;
        while buf[k] == b'9' {
            buf[k] = b'0';
            k -= 1;
        }
        buf[k] = buf[k].wrapping_add(1);
    }
    if buf[0] == b'1' {
        (buf, decpt + 1)
    } else {
        (buf.split_off(1), decpt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Words(Vec<u32>, Vec<(u32, &'static [u8])>);

    impl Args for Words {
        fn word(&mut self) -> u32 {
            self.0.remove(0)
        }

        fn string(&self, at: u32) -> Vec<u8> {
            self.1.iter().find(|(a, _)| *a == at).map(|(_, s)| s.to_vec()).unwrap_or_default()
        }
    }

    fn f(fmt: &str, v: f64) -> String {
        let bits = v.to_bits();
        let mut args = Words(vec![bits as u32, (bits >> 32) as u32], Vec::new());
        String::from_utf8(format(fmt.as_bytes(), &mut args).unwrap()).unwrap()
    }

    #[test]
    fn fixed_follows_msvcr71() {
        assert_eq!(f("%f", 0.0), "0.000000");
        assert_eq!(f("%+.0f", -0.0), "+0");
        assert_eq!(f("%.0f", -0.3), "-0");
        assert_eq!(f("%f", 1.5), "1.500000");
        assert_eq!(f("%f", 6e-7), "0.000001");
        assert_eq!(f("%f", 4e-7), "0.000000");
        assert_eq!(f("%.3f", 999.9996), "1000.000");
        assert_eq!(f("%f", 1e30), "1000000000000000000000000000000.000000");
        assert_eq!(f("%f", 3e9), "3000000000.000000");
        assert_eq!(f("%f", f64::INFINITY), "1.#INF00");
        assert_eq!(f("%.3f", f64::NEG_INFINITY), "-1.#IO");
        assert_eq!(f("%f", f64::from_bits(0xfff8_0000_0000_0000)), "-1.#IND00");
        assert_eq!(f("%f", f64::from_bits(0x7ff8_0000_0000_0000)), "1.#QNAN0");
        assert_eq!(f("%.1f", f64::INFINITY), "1.$");
        assert_eq!(f("%.0f", 2.5), "3");
    }

    #[test]
    fn integers_and_strings() {
        let mut args = Words(vec![0x100, (-7i32) as u32, 5, 0xab, 0x2a, 0, 0x1000], vec![(0x100, b"Emit")]);
        let text = format(b"%s: %d %3d %x %c %s %p%%", &mut args).unwrap();
        assert_eq!(text, b"Emit: -7   5 ab * (null) 00001000%");
    }

    #[test]
    fn unknown_conversions_are_unsupported() {
        let mut args = Words(vec![0, 0], Vec::new());
        assert!(format(b"%e", &mut args).is_err());
    }
}
