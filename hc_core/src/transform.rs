use crate::types::Tone;
use crate::vowel::{
    base_char, compose_vowel, is_vowel, strip_tone_char, vowel_signature, VowelFamily,
};

pub fn apply_double_tap<F>(buffer: &mut String, ch: char, predicate: F) -> bool
where
    F: Fn(char) -> bool,
{
    let mut chars: Vec<char> = buffer.chars().collect();
    let target_lower = ch.to_ascii_lowercase();

    let last_char_matches = if let Some(&last_char) = chars.last() {
        let last_base = base_char(last_char);
        predicate(last_base) && last_base == target_lower
    } else {
        false
    };

    if !last_char_matches {
        let has_ua_context = chars
            .iter()
            .enumerate()
            .rev()
            .find_map(|(idx, &c)| {
                vowel_signature(c).and_then(|(_, _, _)| {
                    let base = base_char(c);
                    if predicate(base) && base == target_lower && idx > 0 {
                        let prev_base = base_char(chars[idx - 1]);
                        if prev_base == 'u' {
                            return Some(true);
                        }
                    }
                    None
                })
            })
            .is_some();

        if !has_ua_context {
            return false;
        }
    }

    // Triple-tap cancel for đ: a 3rd 'd' on an existing đ reverts it and emits
    // the key literally (ddd → dd), mirroring the circumflex toggle (aaa → aa).
    // đ has no vowel_signature, so it cannot use the vowel toggle below.
    if predicate('d') {
        if let Some(&last) = chars.last() {
            let revert = match last {
                'đ' => Some('d'),
                'Đ' => Some('D'),
                _ => None,
            };
            if let Some(plain) = revert {
                let last_idx = chars.len() - 1;
                chars[last_idx] = plain;
                *buffer = chars.into_iter().collect();
                return false;
            }
        }
    }

    {
        let last_char_info = chars.last().and_then(|&last| {
            let last_base = base_char(last);
            if predicate(last_base) && last_base == target_lower {
                vowel_signature(last)
            } else {
                None
            }
        });
        if let Some((family, uppercase, tone)) = last_char_info {
            let last_idx = chars.len() - 1;
            let strip_to_plain = match family {
                VowelFamily::CircumflexA if predicate('a') => Some(VowelFamily::PlainA),
                VowelFamily::CircumflexE if predicate('e') => Some(VowelFamily::PlainE),
                VowelFamily::CircumflexO if predicate('o') => Some(VowelFamily::PlainO),
                _ => None,
            };
            if let Some(plain_family) = strip_to_plain {
                chars[last_idx] = compose_vowel(plain_family, uppercase, tone);
                *buffer = chars.into_iter().collect();
                return false;
            }
        }
    }
    if let Some((idx, replacement)) = chars.iter().enumerate().rev().find_map(|(idx, prev)| {
        let replacement = match vowel_signature(*prev) {
            Some((VowelFamily::PlainA, uppercase, tone)) if predicate('a') => Some(compose_vowel(
                VowelFamily::CircumflexA,
                uppercase || ch.is_uppercase(),
                tone,
            )),
            Some((VowelFamily::PlainE, uppercase, tone)) if predicate('e') => Some(compose_vowel(
                VowelFamily::CircumflexE,
                uppercase || ch.is_uppercase(),
                tone,
            )),
            Some((VowelFamily::PlainO, uppercase, tone)) if predicate('o') => Some(compose_vowel(
                VowelFamily::CircumflexO,
                uppercase || ch.is_uppercase(),
                tone,
            )),
            None if predicate(base_char(*prev)) && matches!(*prev, 'd' | 'D') => {
                Some(if prev.is_uppercase() || ch.is_uppercase() {
                    'Đ'
                } else {
                    'đ'
                })
            }
            _ => None,
        }?;
        Some((idx, replacement))
    }) {
        if chars[idx] == replacement {
            return false;
        }
        chars[idx] = replacement;
        *buffer = chars.into_iter().collect();
        true
    } else {
        false
    }
}

pub fn apply_telex_w(buffer: &mut String) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();

    let (has_u, has_o) =
        chars
            .iter()
            .fold((false, false), |(u, o), &ch| match vowel_signature(ch) {
                Some((VowelFamily::PlainU, _, _)) => (true, o),
                Some((VowelFamily::PlainO, _, _)) => (u, true),
                _ => (u, o),
            });

    if has_u && has_o {
        let changed = apply_horn_to_slice(&mut chars);
        if changed {
            *buffer = chars.into_iter().collect();
            return true;
        }
    }

    // Smart "ua" → "ưa": when the last two vowels form a contiguous "ua" pair
    // and the "u" is NOT preceded by "q"/"Q" (the qu- glide), apply horn to
    // "u" instead of letting the backward scan apply breve to "a".
    if has_u && !has_o {
        if let Some(u_idx) = chars
            .iter()
            .rposition(|ch| vowel_signature(*ch).is_some_and(|(f, _, _)| f == VowelFamily::PlainU))
        {
            let a_idx = u_idx + 1;
            if a_idx < chars.len()
                && vowel_signature(chars[a_idx]).is_some_and(|(f, _, _)| f == VowelFamily::PlainA)
            {
                let preceded_by_q = u_idx > 0 && matches!(chars[u_idx - 1], 'q' | 'Q');
                if !preceded_by_q {
                    let (_, uppercase, tone) = vowel_signature(chars[u_idx]).unwrap();
                    chars[u_idx] = compose_vowel(VowelFamily::HornU, uppercase, tone);
                    *buffer = chars.into_iter().collect();
                    return true;
                }
            }
        }
    }

    for idx in (0..chars.len()).rev() {
        let replacement = match vowel_signature(chars[idx]) {
            Some((VowelFamily::PlainA, uppercase, tone)) => {
                Some(compose_vowel(VowelFamily::BreveA, uppercase, tone))
            }
            Some((VowelFamily::PlainO, uppercase, tone)) => {
                Some(compose_vowel(VowelFamily::HornO, uppercase, tone))
            }
            Some((VowelFamily::PlainU, uppercase, tone)) => {
                Some(compose_vowel(VowelFamily::HornU, uppercase, tone))
            }
            _ => None,
        };
        if let Some(replacement) = replacement {
            chars[idx] = replacement;
            *buffer = chars.into_iter().collect();
            return true;
        }
    }
    false
}

fn apply_diacritic<F>(buffer: &mut String, mapper: F) -> bool
where
    F: Fn(VowelFamily) -> Option<VowelFamily>,
{
    let mut chars: Vec<char> = buffer.chars().collect();

    let existing_tone = chars.iter().find_map(|&ch| {
        vowel_signature(ch).and_then(
            |(_, _, tone)| {
                if tone != Tone::Flat {
                    Some(tone)
                } else {
                    None
                }
            },
        )
    });

    if existing_tone.is_some() {
        chars = chars.iter().map(|&ch| strip_tone_char(ch)).collect();
    }

    let mut applied = false;
    for idx in (0..chars.len()).rev() {
        if let Some((family, uppercase, _)) = vowel_signature(chars[idx]) {
            if let Some(new_family) = mapper(family) {
                chars[idx] = compose_vowel(new_family, uppercase, Tone::Flat);
                applied = true;
                break;
            }
        }
    }

    if !applied {
        let already_has = chars.iter().any(|&ch| {
            if let Some((family, _, _)) = vowel_signature(ch) {
                let base_family = match family {
                    VowelFamily::CircumflexA | VowelFamily::BreveA => VowelFamily::PlainA,
                    VowelFamily::CircumflexE => VowelFamily::PlainE,
                    VowelFamily::CircumflexO | VowelFamily::HornO => VowelFamily::PlainO,
                    VowelFamily::HornU => VowelFamily::PlainU,
                    other => other,
                };
                mapper(base_family) == Some(family)
            } else {
                false
            }
        });
        if already_has {
            return true;
        }
        return false;
    }

    *buffer = chars.into_iter().collect();

    if let Some(tone) = existing_tone {
        apply_tone(buffer, tone, false);
    }

    true
}

pub fn apply_circumflex(buffer: &mut String) -> bool {
    apply_diacritic(buffer, |f| match f {
        VowelFamily::PlainA => Some(VowelFamily::CircumflexA),
        VowelFamily::PlainE => Some(VowelFamily::CircumflexE),
        VowelFamily::PlainO => Some(VowelFamily::CircumflexO),
        _ => None,
    })
}

pub fn apply_horn(buffer: &mut String) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();

    let existing_tone = chars.iter().find_map(|&ch| {
        vowel_signature(ch).and_then(
            |(_, _, tone)| {
                if tone != Tone::Flat {
                    Some(tone)
                } else {
                    None
                }
            },
        )
    });

    if existing_tone.is_some() {
        chars = chars.iter().map(|&ch| strip_tone_char(ch)).collect();
    }

    let changed = apply_horn_to_slice(&mut chars);

    if !changed {
        let already_has = chars.iter().any(|&ch| {
            matches!(
                vowel_signature(ch),
                Some((VowelFamily::HornU | VowelFamily::HornO, _, _))
            )
        });
        return already_has;
    }

    *buffer = chars.into_iter().collect();

    if let Some(tone) = existing_tone {
        apply_tone(buffer, tone, false);
    }

    true
}

fn apply_horn_to_slice(chars: &mut [char]) -> bool {
    let mut changed = false;
    for ch in chars.iter_mut() {
        let replacement = match vowel_signature(*ch) {
            Some((VowelFamily::PlainU, uppercase, tone)) => {
                Some(compose_vowel(VowelFamily::HornU, uppercase, tone))
            }
            Some((VowelFamily::PlainO, uppercase, tone)) => {
                Some(compose_vowel(VowelFamily::HornO, uppercase, tone))
            }
            _ => None,
        };
        if let Some(replacement) = replacement {
            *ch = replacement;
            changed = true;
        }
    }
    changed
}

pub fn apply_breve(buffer: &mut String) -> bool {
    apply_diacritic(buffer, |f| match f {
        VowelFamily::PlainA => Some(VowelFamily::BreveA),
        _ => None,
    })
}

pub fn apply_d_stroke(buffer: &mut String) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();
    for idx in (0..chars.len()).rev() {
        match chars[idx] {
            'd' => {
                chars[idx] = 'đ';
                *buffer = chars.into_iter().collect();
                return true;
            }
            'D' => {
                chars[idx] = 'Đ';
                *buffer = chars.into_iter().collect();
                return true;
            }
            _ => {}
        }
    }
    false
}

pub fn apply_tone(buffer: &mut String, tone: Tone, legacy_tone: bool) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();
    apply_vietnamese_normalization(&mut chars);

    let target = match tone_target_index(&chars, legacy_tone) {
        Some(idx) => idx,
        None => return false,
    };

    // Existing tone anywhere in the syllable → this keystroke is re-toning.
    let existing_tone = chars
        .iter()
        .find_map(|&ch| vowel_signature(ch).and_then(|(_, _, t)| (t != Tone::Flat).then_some(t)));

    if let Some(existing) = existing_tone {
        // Strip the current tone first so re-toning replaces rather than stacks
        // (works for closed syllables too, e.g. "váng" + f → "vàng").
        for ch in chars.iter_mut() {
            *ch = strip_tone_char(*ch);
        }
        if existing == tone {
            // Same tone key pressed again → cancel it and return false so the
            // key is emitted literally (standard Telex/VIQR: "coss" → "cos",
            // "ass" → "as", "toanss" → "toans").
            *buffer = chars.into_iter().collect();
            return false;
        }
        chars[target] = apply_tone_to_char(chars[target], tone);
        *buffer = chars.into_iter().collect();
        return true;
    }

    let base = chars[target];
    let next = apply_tone_to_char(base, tone);
    if next == base {
        return false;
    }
    chars[target] = next;
    *buffer = chars.into_iter().collect();
    true
}

pub fn apply_tone_vni_toggle(buffer: &mut String, tone: Tone, legacy_tone: bool) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();
    apply_vietnamese_normalization(&mut chars);

    // Toggle: if this exact tone is already on any vowel, strip it
    for idx in (0..chars.len()).rev() {
        if let Some((_, _, existing_tone)) = vowel_signature(chars[idx]) {
            if existing_tone == tone {
                let stripped = strip_tone_char(chars[idx]);
                if stripped != chars[idx] {
                    chars[idx] = stripped;
                    *buffer = chars.into_iter().collect();
                    return true;
                }
            }
        }
    }

    // Not already present → apply normally
    apply_tone(buffer, tone, legacy_tone)
}

pub fn apply_circumflex_vni_toggle(buffer: &mut String) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();
    for idx in (0..chars.len()).rev() {
        if let Some((family, uppercase, tone)) = vowel_signature(chars[idx]) {
            let plain = match family {
                VowelFamily::CircumflexA => Some(VowelFamily::PlainA),
                VowelFamily::CircumflexE => Some(VowelFamily::PlainE),
                VowelFamily::CircumflexO => Some(VowelFamily::PlainO),
                _ => None,
            };
            if let Some(plain_family) = plain {
                chars[idx] = compose_vowel(plain_family, uppercase, tone);
                *buffer = chars.into_iter().collect();
                return false;
            }
        }
    }
    apply_circumflex(buffer)
}

pub fn apply_horn_vni_toggle(buffer: &mut String) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();
    let mut stripped = false;
    for idx in (0..chars.len()).rev() {
        if let Some((family, uppercase, tone)) = vowel_signature(chars[idx]) {
            match family {
                VowelFamily::HornU => {
                    chars[idx] = compose_vowel(VowelFamily::PlainU, uppercase, tone);
                    stripped = true;
                }
                VowelFamily::HornO => {
                    chars[idx] = compose_vowel(VowelFamily::PlainO, uppercase, tone);
                    stripped = true;
                }
                _ => {}
            }
        }
    }
    if stripped {
        *buffer = chars.into_iter().collect();
        return false;
    }
    apply_horn(buffer)
}

pub fn apply_breve_vni_toggle(buffer: &mut String) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();
    for idx in (0..chars.len()).rev() {
        if let Some((family, uppercase, tone)) = vowel_signature(chars[idx]) {
            if family == VowelFamily::BreveA {
                chars[idx] = compose_vowel(VowelFamily::PlainA, uppercase, tone);
                *buffer = chars.into_iter().collect();
                return false;
            }
        }
    }
    apply_breve(buffer)
}

pub fn apply_d_stroke_vni_toggle(buffer: &mut String) -> bool {
    let mut chars: Vec<char> = buffer.chars().collect();
    for idx in (0..chars.len()).rev() {
        match chars[idx] {
            'đ' => {
                chars[idx] = 'd';
                *buffer = chars.into_iter().collect();
                return false;
            }
            'Đ' => {
                chars[idx] = 'D';
                *buffer = chars.into_iter().collect();
                return false;
            }
            _ => {}
        }
    }
    apply_d_stroke(buffer)
}

fn apply_vietnamese_normalization(chars: &mut [char]) {
    let bases: String = chars.iter().map(|&ch| base_char(ch)).collect();

    if bases.contains("uay") {
        for idx in 0..chars.len() {
            if let Some((VowelFamily::PlainA, uppercase, tone)) = vowel_signature(chars[idx]) {
                if idx > 0 && idx < chars.len() - 1 {
                    let prev_base = base_char(chars[idx - 1]);
                    let next_base = base_char(chars[idx + 1]);
                    if prev_base == 'u' && next_base == 'y' {
                        chars[idx] = compose_vowel(VowelFamily::CircumflexA, uppercase, tone);
                        break;
                    }
                }
            }
        }
    }

    for idx in 0..chars.len() {
        if let Some((VowelFamily::PlainA, uppercase, tone)) = vowel_signature(chars[idx]) {
            if idx > 0 && idx < chars.len() - 1 {
                let prev_base = base_char(chars[idx - 1]);
                let next_ch = chars[idx + 1];
                // Exclude the "qu" glide: in "qua<coda>" the u belongs to the
                // qu- initial, so the a stays plain (quán, quạt, quát) instead of
                // becoming â. Mirrors the qu exclusion in tone_target_index.
                let qu_glide = idx >= 2 && matches!(chars[idx - 2], 'q' | 'Q');
                if prev_base == 'u' && !is_vowel(next_ch) && !qu_glide {
                    chars[idx] = compose_vowel(VowelFamily::CircumflexA, uppercase, tone);
                    break;
                }
            }
        }
    }

    // NOTE: no "iê/yê → iêu/yêu" auto-completion. "iê"/"yê" are not uniquely
    // followed by u (viết, viên, tiết, yên, yết…), so guessing a trailing u
    // corrupted far more words than it helped (e.g. "viê" + nặng → "việu").
    // The u must be typed, as in every mainstream Telex IME.

    for i in 0..chars.len().saturating_sub(1) {
        if let Some((VowelFamily::HornU, _, _)) = vowel_signature(chars[i]) {
            if i + 1 < chars.len() {
                if let Some((VowelFamily::PlainO, uppercase, tone)) = vowel_signature(chars[i + 1])
                {
                    chars[i + 1] = compose_vowel(VowelFamily::HornO, uppercase, tone);
                    break;
                }
            }
        }
    }
}

fn tone_target_index(chars: &[char], legacy_tone: bool) -> Option<usize> {
    let mut vowels: Vec<usize> = chars
        .iter()
        .enumerate()
        .filter_map(|(idx, ch)| is_vowel(*ch).then_some(idx))
        .collect();

    let &last = vowels.last()?;
    if vowels.len() == 1 {
        return Some(last);
    }

    if vowels.len() >= 2
        && vowels[0] > 0
        && base_char(chars[vowels[0]]) == 'u'
        && matches!(chars[vowels[0] - 1], 'q' | 'Q')
    {
        vowels.remove(0);
    }

    if vowels.len() >= 2
        && vowels[0] > 0
        && base_char(chars[vowels[0]]) == 'i'
        && matches!(chars[vowels[0] - 1], 'g' | 'G')
    {
        vowels.remove(0);
    }

    for preferred in [
        VowelFamily::HornO,
        VowelFamily::CircumflexE,
        VowelFamily::CircumflexO,
        VowelFamily::CircumflexA,
        VowelFamily::BreveA,
        VowelFamily::HornU,
    ] {
        if let Some(idx) = vowels.iter().copied().find(|&idx| {
            vowel_signature(chars[idx]).is_some_and(|(family, _, _)| family == preferred)
        }) {
            return Some(idx);
        }
    }

    if legacy_tone {
        return vowels.first().copied();
    }

    let bases: String = vowels.iter().map(|&idx| base_char(chars[idx])).collect();
    let cluster = bases.as_str();

    match cluster {
        "eu" => return vowels.first().copied(),
        "ieu" | "yeu" => return vowels.get(1).copied().or_else(|| vowels.last().copied()),
        "ai" | "ao" | "au" | "ay" | "eo" | "ia" | "iu" | "oi" | "ua" | "ui" => {
            return vowels.first().copied();
        }
        "oa" | "oe" => return vowels.last().copied(),
        "uy" => {
            // For "uy" cluster: tone on u when no coda, tone on y when there's a coda
            let last_vowel_idx = *vowels.last().unwrap();
            let has_coda = last_vowel_idx < chars.len() - 1;
            if has_coda {
                return vowels.last().copied();
            } else {
                return vowels.first().copied();
            }
        }
        "uo" | "uye" => return vowels.last().copied(),
        "oai" | "uai" | "uay" => return vowels.get(1).copied().or_else(|| vowels.last().copied()),
        _ => {}
    }

    match cluster {
        "ie" | "ye" | "oay" | "uoi" | "ieu" | "yeu" => {
            return vowels.get(1).copied().or_else(|| vowels.last().copied());
        }
        _ => {}
    }

    Some(last)
}

pub fn apply_tone_to_char(ch: char, tone: Tone) -> char {
    match vowel_signature(ch) {
        Some((family, uppercase, _)) => compose_vowel(family, uppercase, tone),
        None => ch,
    }
}

pub fn apply_tone_to_word(word: &mut String, tone: Tone, legacy_tone: bool) -> bool {
    strip_tone_in_place(word);
    apply_tone(word, tone, legacy_tone)
}

fn strip_tone_in_place(word: &mut String) {
    let stripped: String = word.chars().map(strip_tone_char).collect();
    *word = stripped;
}
