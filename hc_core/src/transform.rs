use crate::types::Tone;
use crate::vowel::{
    base_char, compose_vowel, is_vowel, strip_tone_char, vowel_signature, VowelFamily,
};

pub fn apply_double_tap<F>(buffer: &mut String, ch: char, predicate: F) -> bool
where
    F: Fn(char) -> bool,
{
    let mut chars: Vec<char> = buffer.chars().collect();

    if let Some(&last_char) = chars.last() {
        let last_base = base_char(last_char);
        if !predicate(last_base) || last_base != ch.to_ascii_lowercase() {
            return false;
        }
    } else {
        return false;
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
    for idx in (0..chars.len()).rev() {
        if let Some((family, uppercase, tone)) = vowel_signature(chars[idx]) {
            if let Some(new_family) = mapper(family) {
                chars[idx] = compose_vowel(new_family, uppercase, tone);
                *buffer = chars.into_iter().collect();
                return true;
            }
        }
    }
    false
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
    if apply_horn_to_slice(&mut chars) {
        *buffer = chars.into_iter().collect();
        true
    } else {
        false
    }
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
    let target = match tone_target_index(&chars, legacy_tone) {
        Some(idx) => idx,
        None => return false,
    };
    let base = chars[target];
    let next = apply_tone_to_char(base, tone);
    if next == base {
        return false;
    }
    chars[target] = next;
    *buffer = chars.into_iter().collect();
    true
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

    if legacy_tone {
        return vowels.first().copied();
    }

    // In Vietnamese orthography, 'q' is always followed by 'u' as a glide,
    // and 'i' after 'g' is a glide when another vowel follows.
    // The modern tone mark belongs on the vowel after the glide.
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

    let bases: String = vowels.iter().map(|&idx| base_char(chars[idx])).collect();
    let cluster = bases.as_str();

    match cluster {
        "eu" => return vowels.first().copied(),
        "ieu" | "yeu" => return vowels.get(1).copied().or_else(|| vowels.last().copied()),
        "ai" | "ao" | "au" | "ay" | "eo" | "ia" | "iu" | "oi" | "ua" | "ui" => {
            return vowels.first().copied();
        }
        "oa" | "oe" | "uy" => return vowels.last().copied(),
        "uo" | "uye" => return vowels.last().copied(),
        "oai" | "uai" | "uay" => return vowels.get(1).copied().or_else(|| vowels.last().copied()),
        _ => {}
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
