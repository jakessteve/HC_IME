use crate::transform::{
    apply_breve, apply_circumflex, apply_d_stroke, apply_double_tap, apply_horn, apply_telex_w,
    apply_tone,
};
use crate::types::{InputMode, Tone};
use crate::vowel::strip_all_marks;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionMode {
    Inline,
    Dictionary,
}

impl CompositionMode {
    pub fn for_input_mode(mode: InputMode) -> Self {
        match mode {
            InputMode::Telex | InputMode::Vni | InputMode::Viqr => CompositionMode::Inline,
            InputMode::HanNomTelex | InputMode::HanNomVni | InputMode::HanNomViqr => {
                CompositionMode::Dictionary
            }
        }
    }
}

pub struct TypingEngine;

impl TypingEngine {
    pub fn render_raw(raw: &str, mode: InputMode, legacy_tone: bool) -> String {
        let mut rendered = String::new();
        for ch in raw.chars() {
            if !Self::apply_trigger(&mut rendered, mode, ch, legacy_tone) {
                rendered.push(ch);
            }
        }
        rendered
    }

    pub fn apply_trigger(
        buffer: &mut String,
        mode: InputMode,
        ch: char,
        legacy_tone: bool,
    ) -> bool {
        match mode {
            InputMode::Telex | InputMode::HanNomTelex => {
                Self::apply_telex_trigger(buffer, ch, legacy_tone)
            }
            InputMode::Vni | InputMode::HanNomVni => {
                Self::apply_vni_trigger(buffer, ch, legacy_tone)
            }
            InputMode::Viqr | InputMode::HanNomViqr => {
                Self::apply_viqr_trigger(buffer, ch, legacy_tone)
            }
        }
    }

    pub fn apply_telex_trigger(buffer: &mut String, ch: char, legacy_tone: bool) -> bool {
        match ch {
            'z' | 'Z' => {
                let stripped = strip_all_marks(buffer);
                if stripped == buffer.as_str() {
                    false
                } else {
                    *buffer = stripped;
                    true
                }
            }
            's' | 'S' => apply_tone(buffer, Tone::Sac, legacy_tone),
            'f' | 'F' => apply_tone(buffer, Tone::Huyen, legacy_tone),
            'r' | 'R' => apply_tone(buffer, Tone::Hoi, legacy_tone),
            'x' | 'X' => apply_tone(buffer, Tone::Nga, legacy_tone),
            'j' | 'J' => apply_tone(buffer, Tone::Nang, legacy_tone),
            'w' | 'W' => apply_telex_w(buffer),
            'a' | 'A' => apply_double_tap(buffer, ch, |base| base == 'a'),
            'e' | 'E' => apply_double_tap(buffer, ch, |base| base == 'e'),
            'o' | 'O' => apply_double_tap(buffer, ch, |base| base == 'o'),
            'd' | 'D' => apply_double_tap(buffer, ch, |base| base == 'd'),
            _ => false,
        }
    }

    pub fn apply_vni_trigger(buffer: &mut String, ch: char, legacy_tone: bool) -> bool {
        match ch {
            '0' => {
                let stripped = strip_all_marks(buffer);
                if stripped == buffer.as_str() {
                    false
                } else {
                    *buffer = stripped;
                    true
                }
            }
            '1' => apply_tone(buffer, Tone::Sac, legacy_tone),
            '2' => apply_tone(buffer, Tone::Huyen, legacy_tone),
            '3' => apply_tone(buffer, Tone::Hoi, legacy_tone),
            '4' => apply_tone(buffer, Tone::Nga, legacy_tone),
            '5' => apply_tone(buffer, Tone::Nang, legacy_tone),
            '6' => apply_circumflex(buffer),
            '7' => apply_horn(buffer),
            '8' => apply_breve(buffer),
            '9' => apply_d_stroke(buffer),
            _ => false,
        }
    }

    pub fn apply_viqr_trigger(buffer: &mut String, ch: char, legacy_tone: bool) -> bool {
        match ch {
            '\'' => apply_tone(buffer, Tone::Sac, legacy_tone),
            '`' => apply_tone(buffer, Tone::Huyen, legacy_tone),
            '?' => apply_tone(buffer, Tone::Hoi, legacy_tone),
            '~' => apply_tone(buffer, Tone::Nga, legacy_tone),
            '.' => apply_tone(buffer, Tone::Nang, legacy_tone),
            '^' => apply_circumflex(buffer),
            '+' => apply_horn(buffer),
            '(' => apply_breve(buffer),
            'd' | 'D' => apply_double_tap(buffer, ch, |base| base == 'd'),
            _ => false,
        }
    }

    pub fn vni_digit_transforms_buffer(buffer: &str, ch: char, legacy_tone: bool) -> bool {
        if !ch.is_ascii_digit() {
            return false;
        }
        let mut probe = buffer.to_string();
        Self::apply_vni_trigger(&mut probe, ch, legacy_tone)
    }

    pub fn mirror_raw_casing(raw: &str, rendered: &mut String) {
        let raw_alphas: Vec<char> = raw.chars().filter(|ch| ch.is_ascii_alphabetic()).collect();
        if raw_alphas.len() < 2 {
            return;
        }

        let upper_count = raw_alphas.iter().filter(|ch| ch.is_uppercase()).count();
        let lower_count = raw_alphas.iter().filter(|ch| ch.is_lowercase()).count();

        if upper_count >= 2 && lower_count == 0 {
            *rendered = rendered.to_uppercase();
        } else if upper_count == 1
            && raw_alphas[0].is_uppercase()
            && raw_alphas[1..].iter().all(|ch| ch.is_lowercase())
        {
            let mut chars = rendered.chars();
            if let Some(first) = chars.next() {
                let mut result: String = first.to_uppercase().collect();
                for ch in chars {
                    result.extend(ch.to_lowercase());
                }
                *rendered = result;
            }
        }
    }
}
