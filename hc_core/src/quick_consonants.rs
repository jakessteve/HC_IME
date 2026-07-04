pub fn apply_mid_word_quick_consonants(raw: &mut String, lock_pos: &mut usize) {
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() < 2 {
        return;
    }
    let locked = *lock_pos;
    let last = chars[chars.len() - 1];
    let second_last = chars[chars.len() - 2];
    let pair_start = chars.len() - 2;
    if pair_start < locked {
        return;
    }
    let replacement = match (second_last.to_ascii_lowercase(), last.to_ascii_lowercase()) {
        ('c', 'c') => Some(('c', 'h')),
        ('g', 'g') => Some(('g', 'i')),
        ('n', 'n') => Some(('n', 'g')),
        ('u', 'u') => {
            if !has_following_vowel(&chars, chars.len()) {
                Some(('\u{1B0}', '\u{1A1}'))
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some((r1, r2)) = replacement {
        let is_upper = second_last.is_uppercase() && last.is_uppercase();
        let mut new_chars = chars[..pair_start].to_vec();
        if is_upper {
            new_chars.push(r1.to_uppercase().next().unwrap());
            new_chars.push(r2.to_uppercase().next().unwrap());
        } else {
            new_chars.push(r1);
            new_chars.push(r2);
        }
        let new_len = new_chars.len();
        *raw = new_chars.into_iter().collect();
        *lock_pos = new_len;
    }
}

pub fn apply_start_quick_consonants(raw: &mut String, lock_pos: &mut usize) {
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() < 2 || *lock_pos > 0 {
        return;
    }
    let first = chars[0];
    let second = chars[1];
    if !second.is_ascii_alphabetic() {
        return;
    }
    let replacement = match first.to_ascii_lowercase() {
        'f' => Some(('p', 'h')),
        'j' => Some(('g', 'i')),
        'w' => Some(('q', 'u')),
        _ => None,
    };
    if let Some((r1, r2)) = replacement {
        let is_upper = first.is_uppercase() && second.is_uppercase();
        let mut new_chars = vec![r1, r2];
        if is_upper {
            new_chars[0] = r1.to_uppercase().next().unwrap();
            new_chars[1] = r2.to_uppercase().next().unwrap();
        }
        new_chars.extend_from_slice(&chars[1..]);
        let new_len = new_chars.len();
        *raw = new_chars.into_iter().collect();
        *lock_pos = new_len;
    }
}

pub fn apply_end_quick_consonants(raw: &mut String, lock_pos: &mut usize) {
    let chars: Vec<char> = raw.chars().collect();
    if chars.is_empty() {
        return;
    }
    let last_idx = chars.len() - 1;
    if last_idx < *lock_pos {
        return;
    }
    let last = chars[last_idx];
    let replacement = match last.to_ascii_lowercase() {
        'g' => Some(('n', 'g')),
        'h' => Some(('n', 'h')),
        'k' => Some(('c', 'h')),
        _ => None,
    };
    if let Some((r1, r2)) = replacement {
        let is_upper = last.is_uppercase();
        let mut new_chars = chars[..last_idx].to_vec();
        if is_upper {
            new_chars.push(r1.to_uppercase().next().unwrap());
            new_chars.push(r2.to_uppercase().next().unwrap());
        } else {
            new_chars.push(r1);
            new_chars.push(r2);
        }
        let new_len = new_chars.len();
        *raw = new_chars.into_iter().collect();
        *lock_pos = new_len;
    }
}

fn has_following_vowel(chars: &[char], from: usize) -> bool {
    chars[from..]
        .iter()
        .any(|ch| matches!(ch.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y'))
}
