use crate::types::Tone;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VowelFamily {
    PlainA,
    BreveA,
    CircumflexA,
    PlainE,
    CircumflexE,
    PlainI,
    PlainO,
    CircumflexO,
    HornO,
    PlainU,
    HornU,
    PlainY,
}

pub fn is_vowel(ch: char) -> bool {
    vowel_signature(ch).is_some()
}

pub fn vowel_signature(ch: char) -> Option<(VowelFamily, bool, Tone)> {
    let uppercase = ch.is_uppercase();
    let (family, tone) = match ch {
        'a' | 'A' => (VowelFamily::PlainA, Tone::Flat),
        'á' | 'Á' => (VowelFamily::PlainA, Tone::Sac),
        'à' | 'À' => (VowelFamily::PlainA, Tone::Huyen),
        'ả' | 'Ả' => (VowelFamily::PlainA, Tone::Hoi),
        'ã' | 'Ã' => (VowelFamily::PlainA, Tone::Nga),
        'ạ' | 'Ạ' => (VowelFamily::PlainA, Tone::Nang),
        'ă' | 'Ă' => (VowelFamily::BreveA, Tone::Flat),
        'ắ' | 'Ắ' => (VowelFamily::BreveA, Tone::Sac),
        'ằ' | 'Ằ' => (VowelFamily::BreveA, Tone::Huyen),
        'ẳ' | 'Ẳ' => (VowelFamily::BreveA, Tone::Hoi),
        'ẵ' | 'Ẵ' => (VowelFamily::BreveA, Tone::Nga),
        'ặ' | 'Ặ' => (VowelFamily::BreveA, Tone::Nang),
        'â' | 'Â' => (VowelFamily::CircumflexA, Tone::Flat),
        'ấ' | 'Ấ' => (VowelFamily::CircumflexA, Tone::Sac),
        'ầ' | 'Ầ' => (VowelFamily::CircumflexA, Tone::Huyen),
        'ẩ' | 'Ẩ' => (VowelFamily::CircumflexA, Tone::Hoi),
        'ẫ' | 'Ẫ' => (VowelFamily::CircumflexA, Tone::Nga),
        'ậ' | 'Ậ' => (VowelFamily::CircumflexA, Tone::Nang),
        'e' | 'E' => (VowelFamily::PlainE, Tone::Flat),
        'é' | 'É' => (VowelFamily::PlainE, Tone::Sac),
        'è' | 'È' => (VowelFamily::PlainE, Tone::Huyen),
        'ẻ' | 'Ẻ' => (VowelFamily::PlainE, Tone::Hoi),
        'ẽ' | 'Ẽ' => (VowelFamily::PlainE, Tone::Nga),
        'ẹ' | 'Ẹ' => (VowelFamily::PlainE, Tone::Nang),
        'ê' | 'Ê' => (VowelFamily::CircumflexE, Tone::Flat),
        'ế' | 'Ế' => (VowelFamily::CircumflexE, Tone::Sac),
        'ề' | 'Ề' => (VowelFamily::CircumflexE, Tone::Huyen),
        'ể' | 'Ể' => (VowelFamily::CircumflexE, Tone::Hoi),
        'ễ' | 'Ễ' => (VowelFamily::CircumflexE, Tone::Nga),
        'ệ' | 'Ệ' => (VowelFamily::CircumflexE, Tone::Nang),
        'i' | 'I' => (VowelFamily::PlainI, Tone::Flat),
        'í' | 'Í' => (VowelFamily::PlainI, Tone::Sac),
        'ì' | 'Ì' => (VowelFamily::PlainI, Tone::Huyen),
        'ỉ' | 'Ỉ' => (VowelFamily::PlainI, Tone::Hoi),
        'ĩ' | 'Ĩ' => (VowelFamily::PlainI, Tone::Nga),
        'ị' | 'Ị' => (VowelFamily::PlainI, Tone::Nang),
        'o' | 'O' => (VowelFamily::PlainO, Tone::Flat),
        'ó' | 'Ó' => (VowelFamily::PlainO, Tone::Sac),
        'ò' | 'Ò' => (VowelFamily::PlainO, Tone::Huyen),
        'ỏ' | 'Ỏ' => (VowelFamily::PlainO, Tone::Hoi),
        'õ' | 'Õ' => (VowelFamily::PlainO, Tone::Nga),
        'ọ' | 'Ọ' => (VowelFamily::PlainO, Tone::Nang),
        'ô' | 'Ô' => (VowelFamily::CircumflexO, Tone::Flat),
        'ố' | 'Ố' => (VowelFamily::CircumflexO, Tone::Sac),
        'ồ' | 'Ồ' => (VowelFamily::CircumflexO, Tone::Huyen),
        'ổ' | 'Ổ' => (VowelFamily::CircumflexO, Tone::Hoi),
        'ỗ' | 'Ỗ' => (VowelFamily::CircumflexO, Tone::Nga),
        'ộ' | 'Ộ' => (VowelFamily::CircumflexO, Tone::Nang),
        'ơ' | 'Ơ' => (VowelFamily::HornO, Tone::Flat),
        'ớ' | 'Ớ' => (VowelFamily::HornO, Tone::Sac),
        'ờ' | 'Ờ' => (VowelFamily::HornO, Tone::Huyen),
        'ở' | 'Ở' => (VowelFamily::HornO, Tone::Hoi),
        'ỡ' | 'Ỡ' => (VowelFamily::HornO, Tone::Nga),
        'ợ' | 'Ợ' => (VowelFamily::HornO, Tone::Nang),
        'u' | 'U' => (VowelFamily::PlainU, Tone::Flat),
        'ú' | 'Ú' => (VowelFamily::PlainU, Tone::Sac),
        'ù' | 'Ù' => (VowelFamily::PlainU, Tone::Huyen),
        'ủ' | 'Ủ' => (VowelFamily::PlainU, Tone::Hoi),
        'ũ' | 'Ũ' => (VowelFamily::PlainU, Tone::Nga),
        'ụ' | 'Ụ' => (VowelFamily::PlainU, Tone::Nang),
        'ư' | 'Ư' => (VowelFamily::HornU, Tone::Flat),
        'ứ' | 'Ứ' => (VowelFamily::HornU, Tone::Sac),
        'ừ' | 'Ừ' => (VowelFamily::HornU, Tone::Huyen),
        'ử' | 'Ử' => (VowelFamily::HornU, Tone::Hoi),
        'ữ' | 'Ữ' => (VowelFamily::HornU, Tone::Nga),
        'ự' | 'Ự' => (VowelFamily::HornU, Tone::Nang),
        'y' | 'Y' => (VowelFamily::PlainY, Tone::Flat),
        'ý' | 'Ý' => (VowelFamily::PlainY, Tone::Sac),
        'ỳ' | 'Ỳ' => (VowelFamily::PlainY, Tone::Huyen),
        'ỷ' | 'Ỷ' => (VowelFamily::PlainY, Tone::Hoi),
        'ỹ' | 'Ỹ' => (VowelFamily::PlainY, Tone::Nga),
        'ỵ' | 'Ỵ' => (VowelFamily::PlainY, Tone::Nang),
        _ => return None,
    };
    Some((family, uppercase, tone))
}

pub fn compose_vowel(family: VowelFamily, uppercase: bool, tone: Tone) -> char {
    match (family, uppercase, tone) {
        (VowelFamily::PlainA, false, Tone::Flat) => 'a',
        (VowelFamily::PlainA, false, Tone::Sac) => 'á',
        (VowelFamily::PlainA, false, Tone::Huyen) => 'à',
        (VowelFamily::PlainA, false, Tone::Hoi) => 'ả',
        (VowelFamily::PlainA, false, Tone::Nga) => 'ã',
        (VowelFamily::PlainA, false, Tone::Nang) => 'ạ',
        (VowelFamily::PlainA, true, Tone::Flat) => 'A',
        (VowelFamily::PlainA, true, Tone::Sac) => 'Á',
        (VowelFamily::PlainA, true, Tone::Huyen) => 'À',
        (VowelFamily::PlainA, true, Tone::Hoi) => 'Ả',
        (VowelFamily::PlainA, true, Tone::Nga) => 'Ã',
        (VowelFamily::PlainA, true, Tone::Nang) => 'Ạ',
        (VowelFamily::BreveA, false, Tone::Flat) => 'ă',
        (VowelFamily::BreveA, false, Tone::Sac) => 'ắ',
        (VowelFamily::BreveA, false, Tone::Huyen) => 'ằ',
        (VowelFamily::BreveA, false, Tone::Hoi) => 'ẳ',
        (VowelFamily::BreveA, false, Tone::Nga) => 'ẵ',
        (VowelFamily::BreveA, false, Tone::Nang) => 'ặ',
        (VowelFamily::BreveA, true, Tone::Flat) => 'Ă',
        (VowelFamily::BreveA, true, Tone::Sac) => 'Ắ',
        (VowelFamily::BreveA, true, Tone::Huyen) => 'Ằ',
        (VowelFamily::BreveA, true, Tone::Hoi) => 'Ẳ',
        (VowelFamily::BreveA, true, Tone::Nga) => 'Ẵ',
        (VowelFamily::BreveA, true, Tone::Nang) => 'Ặ',
        (VowelFamily::CircumflexA, false, Tone::Flat) => 'â',
        (VowelFamily::CircumflexA, false, Tone::Sac) => 'ấ',
        (VowelFamily::CircumflexA, false, Tone::Huyen) => 'ầ',
        (VowelFamily::CircumflexA, false, Tone::Hoi) => 'ẩ',
        (VowelFamily::CircumflexA, false, Tone::Nga) => 'ẫ',
        (VowelFamily::CircumflexA, false, Tone::Nang) => 'ậ',
        (VowelFamily::CircumflexA, true, Tone::Flat) => 'Â',
        (VowelFamily::CircumflexA, true, Tone::Sac) => 'Ấ',
        (VowelFamily::CircumflexA, true, Tone::Huyen) => 'Ầ',
        (VowelFamily::CircumflexA, true, Tone::Hoi) => 'Ẩ',
        (VowelFamily::CircumflexA, true, Tone::Nga) => 'Ẫ',
        (VowelFamily::CircumflexA, true, Tone::Nang) => 'Ậ',
        (VowelFamily::PlainE, false, Tone::Flat) => 'e',
        (VowelFamily::PlainE, false, Tone::Sac) => 'é',
        (VowelFamily::PlainE, false, Tone::Huyen) => 'è',
        (VowelFamily::PlainE, false, Tone::Hoi) => 'ẻ',
        (VowelFamily::PlainE, false, Tone::Nga) => 'ẽ',
        (VowelFamily::PlainE, false, Tone::Nang) => 'ẹ',
        (VowelFamily::PlainE, true, Tone::Flat) => 'E',
        (VowelFamily::PlainE, true, Tone::Sac) => 'É',
        (VowelFamily::PlainE, true, Tone::Huyen) => 'È',
        (VowelFamily::PlainE, true, Tone::Hoi) => 'Ẻ',
        (VowelFamily::PlainE, true, Tone::Nga) => 'Ẽ',
        (VowelFamily::PlainE, true, Tone::Nang) => 'Ẹ',
        (VowelFamily::CircumflexE, false, Tone::Flat) => 'ê',
        (VowelFamily::CircumflexE, false, Tone::Sac) => 'ế',
        (VowelFamily::CircumflexE, false, Tone::Huyen) => 'ề',
        (VowelFamily::CircumflexE, false, Tone::Hoi) => 'ể',
        (VowelFamily::CircumflexE, false, Tone::Nga) => 'ễ',
        (VowelFamily::CircumflexE, false, Tone::Nang) => 'ệ',
        (VowelFamily::CircumflexE, true, Tone::Flat) => 'Ê',
        (VowelFamily::CircumflexE, true, Tone::Sac) => 'Ế',
        (VowelFamily::CircumflexE, true, Tone::Huyen) => 'Ề',
        (VowelFamily::CircumflexE, true, Tone::Hoi) => 'Ể',
        (VowelFamily::CircumflexE, true, Tone::Nga) => 'Ễ',
        (VowelFamily::CircumflexE, true, Tone::Nang) => 'Ệ',
        (VowelFamily::PlainI, false, Tone::Flat) => 'i',
        (VowelFamily::PlainI, false, Tone::Sac) => 'í',
        (VowelFamily::PlainI, false, Tone::Huyen) => 'ì',
        (VowelFamily::PlainI, false, Tone::Hoi) => 'ỉ',
        (VowelFamily::PlainI, false, Tone::Nga) => 'ĩ',
        (VowelFamily::PlainI, false, Tone::Nang) => 'ị',
        (VowelFamily::PlainI, true, Tone::Flat) => 'I',
        (VowelFamily::PlainI, true, Tone::Sac) => 'Í',
        (VowelFamily::PlainI, true, Tone::Huyen) => 'Ì',
        (VowelFamily::PlainI, true, Tone::Hoi) => 'Ỉ',
        (VowelFamily::PlainI, true, Tone::Nga) => 'Ĩ',
        (VowelFamily::PlainI, true, Tone::Nang) => 'Ị',
        (VowelFamily::PlainO, false, Tone::Flat) => 'o',
        (VowelFamily::PlainO, false, Tone::Sac) => 'ó',
        (VowelFamily::PlainO, false, Tone::Huyen) => 'ò',
        (VowelFamily::PlainO, false, Tone::Hoi) => 'ỏ',
        (VowelFamily::PlainO, false, Tone::Nga) => 'õ',
        (VowelFamily::PlainO, false, Tone::Nang) => 'ọ',
        (VowelFamily::PlainO, true, Tone::Flat) => 'O',
        (VowelFamily::PlainO, true, Tone::Sac) => 'Ó',
        (VowelFamily::PlainO, true, Tone::Huyen) => 'Ò',
        (VowelFamily::PlainO, true, Tone::Hoi) => 'Ỏ',
        (VowelFamily::PlainO, true, Tone::Nga) => 'Õ',
        (VowelFamily::PlainO, true, Tone::Nang) => 'Ọ',
        (VowelFamily::CircumflexO, false, Tone::Flat) => 'ô',
        (VowelFamily::CircumflexO, false, Tone::Sac) => 'ố',
        (VowelFamily::CircumflexO, false, Tone::Huyen) => 'ồ',
        (VowelFamily::CircumflexO, false, Tone::Hoi) => 'ổ',
        (VowelFamily::CircumflexO, false, Tone::Nga) => 'ỗ',
        (VowelFamily::CircumflexO, false, Tone::Nang) => 'ộ',
        (VowelFamily::CircumflexO, true, Tone::Flat) => 'Ô',
        (VowelFamily::CircumflexO, true, Tone::Sac) => 'Ố',
        (VowelFamily::CircumflexO, true, Tone::Huyen) => 'Ồ',
        (VowelFamily::CircumflexO, true, Tone::Hoi) => 'Ổ',
        (VowelFamily::CircumflexO, true, Tone::Nga) => 'Ỗ',
        (VowelFamily::CircumflexO, true, Tone::Nang) => 'Ộ',
        (VowelFamily::HornO, false, Tone::Flat) => 'ơ',
        (VowelFamily::HornO, false, Tone::Sac) => 'ớ',
        (VowelFamily::HornO, false, Tone::Huyen) => 'ờ',
        (VowelFamily::HornO, false, Tone::Hoi) => 'ở',
        (VowelFamily::HornO, false, Tone::Nga) => 'ỡ',
        (VowelFamily::HornO, false, Tone::Nang) => 'ợ',
        (VowelFamily::HornO, true, Tone::Flat) => 'Ơ',
        (VowelFamily::HornO, true, Tone::Sac) => 'Ớ',
        (VowelFamily::HornO, true, Tone::Huyen) => 'Ờ',
        (VowelFamily::HornO, true, Tone::Hoi) => 'Ở',
        (VowelFamily::HornO, true, Tone::Nga) => 'Ỡ',
        (VowelFamily::HornO, true, Tone::Nang) => 'Ợ',
        (VowelFamily::PlainU, false, Tone::Flat) => 'u',
        (VowelFamily::PlainU, false, Tone::Sac) => 'ú',
        (VowelFamily::PlainU, false, Tone::Huyen) => 'ù',
        (VowelFamily::PlainU, false, Tone::Hoi) => 'ủ',
        (VowelFamily::PlainU, false, Tone::Nga) => 'ũ',
        (VowelFamily::PlainU, false, Tone::Nang) => 'ụ',
        (VowelFamily::PlainU, true, Tone::Flat) => 'U',
        (VowelFamily::PlainU, true, Tone::Sac) => 'Ú',
        (VowelFamily::PlainU, true, Tone::Huyen) => 'Ù',
        (VowelFamily::PlainU, true, Tone::Hoi) => 'Ủ',
        (VowelFamily::PlainU, true, Tone::Nga) => 'Ũ',
        (VowelFamily::PlainU, true, Tone::Nang) => 'Ụ',
        (VowelFamily::HornU, false, Tone::Flat) => 'ư',
        (VowelFamily::HornU, false, Tone::Sac) => 'ứ',
        (VowelFamily::HornU, false, Tone::Huyen) => 'ừ',
        (VowelFamily::HornU, false, Tone::Hoi) => 'ử',
        (VowelFamily::HornU, false, Tone::Nga) => 'ữ',
        (VowelFamily::HornU, false, Tone::Nang) => 'ự',
        (VowelFamily::HornU, true, Tone::Flat) => 'Ư',
        (VowelFamily::HornU, true, Tone::Sac) => 'Ứ',
        (VowelFamily::HornU, true, Tone::Huyen) => 'Ừ',
        (VowelFamily::HornU, true, Tone::Hoi) => 'Ử',
        (VowelFamily::HornU, true, Tone::Nga) => 'Ữ',
        (VowelFamily::HornU, true, Tone::Nang) => 'Ự',
        (VowelFamily::PlainY, false, Tone::Flat) => 'y',
        (VowelFamily::PlainY, false, Tone::Sac) => 'ý',
        (VowelFamily::PlainY, false, Tone::Huyen) => 'ỳ',
        (VowelFamily::PlainY, false, Tone::Hoi) => 'ỷ',
        (VowelFamily::PlainY, false, Tone::Nga) => 'ỹ',
        (VowelFamily::PlainY, false, Tone::Nang) => 'ỵ',
        (VowelFamily::PlainY, true, Tone::Flat) => 'Y',
        (VowelFamily::PlainY, true, Tone::Sac) => 'Ý',
        (VowelFamily::PlainY, true, Tone::Huyen) => 'Ỳ',
        (VowelFamily::PlainY, true, Tone::Hoi) => 'Ỷ',
        (VowelFamily::PlainY, true, Tone::Nga) => 'Ỹ',
        (VowelFamily::PlainY, true, Tone::Nang) => 'Ỵ',
    }
}

pub fn base_char(ch: char) -> char {
    match vowel_signature(ch) {
        Some((VowelFamily::PlainA | VowelFamily::BreveA | VowelFamily::CircumflexA, _, _)) => 'a',
        Some((VowelFamily::PlainE | VowelFamily::CircumflexE, _, _)) => 'e',
        Some((VowelFamily::PlainI, _, _)) => 'i',
        Some((VowelFamily::PlainO | VowelFamily::CircumflexO | VowelFamily::HornO, _, _)) => 'o',
        Some((VowelFamily::PlainU | VowelFamily::HornU, _, _)) => 'u',
        Some((VowelFamily::PlainY, _, _)) => 'y',
        None if matches!(ch, 'đ' | 'Đ') => 'd',
        None => ch.to_ascii_lowercase(),
    }
}

pub fn plain_char_preserve_case(ch: char) -> char {
    match vowel_signature(ch) {
        Some((VowelFamily::PlainA | VowelFamily::BreveA | VowelFamily::CircumflexA, true, _)) => {
            'A'
        }
        Some((VowelFamily::PlainA | VowelFamily::BreveA | VowelFamily::CircumflexA, false, _)) => {
            'a'
        }
        Some((VowelFamily::PlainE | VowelFamily::CircumflexE, true, _)) => 'E',
        Some((VowelFamily::PlainE | VowelFamily::CircumflexE, false, _)) => 'e',
        Some((VowelFamily::PlainI, true, _)) => 'I',
        Some((VowelFamily::PlainI, false, _)) => 'i',
        Some((VowelFamily::PlainO | VowelFamily::CircumflexO | VowelFamily::HornO, true, _)) => 'O',
        Some((VowelFamily::PlainO | VowelFamily::CircumflexO | VowelFamily::HornO, false, _)) => {
            'o'
        }
        Some((VowelFamily::PlainU | VowelFamily::HornU, true, _)) => 'U',
        Some((VowelFamily::PlainU | VowelFamily::HornU, false, _)) => 'u',
        Some((VowelFamily::PlainY, true, _)) => 'Y',
        Some((VowelFamily::PlainY, false, _)) => 'y',
        None if ch == 'đ' => 'd',
        None if ch == 'Đ' => 'D',
        None => ch,
    }
}

pub fn strip_tone_char(ch: char) -> char {
    match vowel_signature(ch) {
        Some((family, uppercase, _)) => compose_vowel(family, uppercase, Tone::Flat),
        None => ch,
    }
}

pub fn strip_marks_ascii_lower(input: &str) -> String {
    input
        .chars()
        .map(base_char)
        .collect::<String>()
        .to_ascii_lowercase()
}

pub fn strip_all_marks(input: &str) -> String {
    input.chars().map(plain_char_preserve_case).collect()
}

pub fn has_vietnamese_mark(input: &str) -> bool {
    input.chars().any(|ch| {
        matches!(ch, 'đ' | 'Đ')
            || vowel_signature(ch).is_some_and(|(family, _, tone)| {
                tone != Tone::Flat
                    || !matches!(
                        family,
                        VowelFamily::PlainA
                            | VowelFamily::PlainE
                            | VowelFamily::PlainI
                            | VowelFamily::PlainO
                            | VowelFamily::PlainU
                            | VowelFamily::PlainY
                    )
            })
    })
}
