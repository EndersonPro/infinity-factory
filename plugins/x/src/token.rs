//! The `token` query parameter the syndication endpoint carries.
//!
//! The value is `((id / 1e15) * π).toString(36)` with every `0` and `.`
//! removed, so producing it needs JavaScript's float-to-radix conversion —
//! ECMAScript 5 section 9.8.1. That algorithm is reimplemented here from the
//! specification; it is public, and yt-dlp's implementation of the same
//! specification serves only as a test oracle.
//!
//! The endpoint was measured not to validate this value: a nonsense token, and
//! no token at all, both return the same body. It is derived anyway. A fixed
//! string shared by every installation is a trivial signature to filter, while
//! a derived one varies per post exactly as a browser's does — and if the
//! endpoint ever starts checking, the request shape is already correct.

const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const RADIX: u32 = 36;

/// The gap between `value` and the next representable `f64` above it.
///
/// Python's `math.ulp`, computed from the bit pattern because `f64::next_up`
/// is not stable. The ES5 loop terminates on this quantity: below it the
/// remaining fraction is noise, and emitting further digits would invent
/// precision the double never carried.
fn ulp(value: f64) -> f64 {
    let value = value.abs();
    if value == 0.0 {
        return f64::from_bits(1);
    }
    if !value.is_finite() {
        return value;
    }
    f64::from_bits(value.to_bits() + 1) - value
}

/// `Number.prototype.toString(36)` for a finite `f64`, per ES5 9.8.1.
fn js_number_to_base36(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.into();
    }

    let negative = value < 0.0;
    let value = value.abs();
    let radix = f64::from(RADIX);

    let mut integer = value.trunc();
    let mut fraction = value.fract();
    let mut delta = f64::from_bits(1).max(ulp(value) / 2.0);

    let mut fractional: Vec<u32> = Vec::new();
    let has_fraction = fraction >= delta;

    while fraction >= delta {
        delta *= radix;
        let scaled = fraction * radix;
        let digit = scaled.trunc();
        fraction = scaled.fract();
        fractional.push(digit as u32);

        // Round half to even, then carry leftwards. A carry running off the
        // front of the fraction lands on the integer part.
        let digit_is_odd = (digit as u64) & 1 == 1;
        if (fraction > 0.5 || (fraction == 0.5 && digit_is_odd)) && fraction + delta > 1.0 {
            let mut carried = false;
            for index in (0..fractional.len()).rev() {
                if fractional[index] + 1 < RADIX {
                    fractional[index] += 1;
                    carried = true;
                    break;
                }
                fractional.pop();
            }
            if !carried {
                integer += 1.0;
            }
            break;
        }
    }

    let mut digits: Vec<u32> = Vec::new();
    let mut remaining = integer as u128;
    loop {
        digits.push((remaining % u128::from(RADIX)) as u32);
        remaining /= u128::from(RADIX);
        if remaining == 0 {
            break;
        }
    }
    digits.reverse();

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    for digit in digits {
        out.push(ALPHABET[digit as usize] as char);
    }
    if has_fraction && !fractional.is_empty() {
        out.push('.');
        for digit in fractional {
            out.push(ALPHABET[digit as usize] as char);
        }
    }
    out
}

/// The token for `post_id`, with `0` and `.` stripped as the site does.
pub fn syndication_token(post_id: u64) -> String {
    js_number_to_base36((post_id as f64 / 1e15) * std::f64::consts::PI)
        .chars()
        .filter(|value| *value != '0' && *value != '.')
        .collect()
}
