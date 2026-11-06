//! How the game writes values, and how the OCR tends to misread them.

/// A quality: "523Q", "523 Q", "5230", "523.0", "371g" (the dim Q read as a zero, a dot or a letter) -> 523.
/// Second value: ambiguous ("1000" may be 100Q or 1000; a quality is 1–1000). Without the Q at least three
/// digits ("31" is a cut-off text, not a quality).
pub fn quality(text: &str) -> Option<(u32, bool)> {
    let t: String = text.chars().filter(|c| !c.is_whitespace() && !matches!(c, '.' | '-' | ':' | ',')).collect();
    let (digits, suffix) = match t.chars().last()? {
        c if c.is_ascii_digit() => (t.as_str(), None),
        // Any letter but the count's x
        c if c.is_alphabetic() && !matches!(c, 'x' | 'X') => (&t[..t.len() - c.len_utf8()], Some(c)),
        _ => return None,
    };
    // No leading zero: "0.263" (an SCU amount) is no quality
    if digits.is_empty() || digits.len() > 5 || !digits.chars().all(|c| c.is_ascii_digit()) || digits.starts_with('0') {
        return None;
    }
    let value: u32 = digits.parse().ok()?;
    if suffix.is_some() {
        if (1..=1000).contains(&value) {
            return Some((value, false));
        }
        // "3100Q": the dim Q read as a 0 and then the Q itself
        if digits.len() == 4 && digits.ends_with('0') {
            let v: u32 = digits[..3].parse().ok()?;
            return Some((v, false));
        }
        return None;
    }
    if digits.len() < 3 {
        return None;
    }
    if value <= 1000 {
        // 4 digits ending in 0 could be a 3-digit quality with the Q read as 0
        return Some((value, digits.len() == 4 && digits.ends_with('0')));
    }
    // "5230": the trailing Q became a 0
    if let Some(rest) = digits.strip_suffix('0') {
        let v: u32 = rest.parse().ok()?;
        return (1..=1000).contains(&v).then_some((v, false));
    }
    None
}

/// An SCU amount: "0.263 SCU", "2 sCu", "0,5 SCU" -> "0.263", "2", "0.5" (as text: amounts are exact decimals).
pub fn scu(text: &str) -> Option<String> {
    let t = text.trim();
    let n = t.len().checked_sub(3)?;
    if !t.is_char_boundary(n) || !t[n..].eq_ignore_ascii_case("scu") {
        return None;
    }
    let number: String = t[..n].trim().chars().map(|c| if c == ',' { '.' } else { c }).collect();
    let ok = !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
        && number.matches('.').count() <= 1
        && number.chars().any(|c| c.is_ascii_digit());
    ok.then_some(number)
}

/// A stack count on an item tile: "2x" (also "2 x", "12X", "2×") -> 2; a count below 2 or without the x is none.
pub fn stack_count(text: &str) -> Option<u32> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let digits = t.strip_suffix(['x', 'X', '×'])?;
    let n = digits.parse::<u32>().ok()?;
    (n >= 2).then_some(n)
}

/// A duration: "57m 54s", "1h 2m 3s", "0m 36s" -> seconds.
pub fn duration(text: &str) -> Option<u32> {
    let mut total = 0u32;
    let mut number = String::new();
    let mut any = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            number.push(c);
        } else if matches!(c, 'h' | 'm' | 's' | 'H' | 'M' | 'S') {
            let n: u32 = number.parse().ok()?;
            total += n * match c.to_ascii_lowercase() {
                'h' => 3600,
                'm' => 60,
                _ => 1,
            };
            number.clear();
            any = true;
        } else if !c.is_whitespace() {
            return None;
        }
    }
    (any && number.is_empty()).then_some(total)
}

/// A whole number ("175"); "--" and anything else is none.
pub fn number(text: &str) -> Option<u32> {
    let t = text.trim();
    (!t.is_empty() && t.chars().all(|c| c.is_ascii_digit())).then(|| t.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualities() {
        assert_eq!(quality("523Q"), Some((523, false)));
        assert_eq!(quality("523 Q"), Some((523, false)));
        assert_eq!(quality("5230"), Some((523, false)));
        assert_eq!(quality("1000"), Some((1000, true)));
        assert_eq!(quality("1000Q"), Some((1000, false)));
        assert_eq!(quality("10000"), Some((1000, false)));
        assert_eq!(quality("686"), Some((686, false)));
        assert_eq!(quality("523.0"), Some((523, false)));
        assert_eq!(quality("371g"), Some((371, false)));
        assert_eq!(quality("31Q"), Some((31, false)));
        assert_eq!(quality("3100Q"), Some((310, false)));
        assert_eq!(quality("31"), None, "a cut-off text");
        assert_eq!(quality("30x"), None);
        assert_eq!(quality("0"), None);
        assert_eq!(quality("0.263"), None);
        assert_eq!(quality("4x"), None);
        assert_eq!(quality("1234Q"), None);
    }

    #[test]
    fn scu_amounts() {
        assert_eq!(scu("0.263 sCu"), Some("0.263".into()));
        assert_eq!(scu("2 SCU"), Some("2".into()));
        assert_eq!(scu("0,5 SCU"), Some("0.5".into()));
        assert_eq!(scu("10/5000 scu"), None);
        assert_eq!(scu("SCU"), None);
        assert_eq!(scu("µ"), None);
    }

    #[test]
    fn stack_counts() {
        assert_eq!(stack_count("2x"), Some(2));
        assert_eq!(stack_count("12 X"), Some(12));
        assert_eq!(stack_count("3×"), Some(3));
        assert_eq!(stack_count("1x"), None);
        assert_eq!(stack_count("2"), None, "no x: an ammo count, a slot number");
        assert_eq!(stack_count("x2"), None);
    }

    #[test]
    fn durations() {
        assert_eq!(duration("57m 54s"), Some(3474));
        assert_eq!(duration("0m 36s"), Some(36));
        assert_eq!(duration("1h 2m 3s"), Some(3723));
        assert_eq!(duration("--"), None);
        assert_eq!(duration("12"), None);
        assert_eq!(duration("CONFIRM"), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(number(" 175 "), Some(175));
        assert_eq!(number("--"), None);
        assert_eq!(number(""), None);
    }
}
