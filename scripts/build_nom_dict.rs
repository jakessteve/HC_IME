use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DictEntry {
    pub reading: String,
    pub candidates: Vec<char>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PhraseEntry {
    reading: String,
    glyphs: String,
    system_rank: u32,
}

fn normalize_phrase_reading(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn parse_unihan(path: &Path) -> Result<HashMap<String, Vec<char>>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut map: HashMap<String, Vec<char>> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 && parts[1] == "kVietnamese" {
            let codepoint_str = parts[0].trim_start_matches("U+");
            if let Ok(cp) = u32::from_str_radix(codepoint_str, 16) {
                if let Some(ch) = char::from_u32(cp) {
                    for raw_reading in parts[2].split_whitespace() {
                        let reading = raw_reading.to_lowercase();
                        if !reading.is_empty() {
                            let entry = map.entry(reading).or_default();
                            if !entry.contains(&ch) {
                                entry.push(ch);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(map)
}

struct NomStdRow {
    reading: String,
    chunom: String,
}

fn parse_nom_standardization(
    path: &Path,
) -> Result<(Vec<NomStdRow>, usize), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    let mut skipped_empty_count = 0;

    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        if idx == 0 || line.trim().is_empty() {
            continue; // Header or empty
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 4 {
            let qnc = parts[2].trim();
            let chunom = parts[3].trim();
            if chunom.is_empty() || chunom == "？" || chunom.contains('？') {
                skipped_empty_count += 1;
                continue;
            }

            for r in qnc.split_whitespace() {
                let reading = r.to_lowercase();
                if !reading.is_empty() {
                    rows.push(NomStdRow {
                        reading,
                        chunom: chunom.to_string(),
                    });
                }
            }
        }
    }

    Ok((rows, skipped_empty_count))
}

fn parse_rime_dict(path: &Path) -> Result<HashMap<String, Vec<char>>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut map: HashMap<String, Vec<char>> = HashMap::new();
    let mut header_ended = false;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "..." {
            header_ended = true;
            continue;
        }
        if trimmed == "---" || (!header_ended && trimmed.contains(':')) {
            continue;
        }

        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() >= 2 {
            let word = parts[0].trim();
            let reading = parts[1].trim().to_lowercase();

            if word.chars().count() == 1 && !reading.is_empty() {
                let ch = word.chars().next().unwrap();
                let entry = map.entry(reading).or_default();
                if !entry.contains(&ch) {
                    entry.push(ch);
                }
            }
        }
    }

    Ok(map)
}

/// Reads the bundled compound section without changing the legacy single-glyph
/// dictionary. Compound rows must be tab-delimited so accidental prose does not
/// silently become a prediction.
fn parse_rime_phrases(path: &Path) -> Result<Vec<PhraseEntry>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut phrases = Vec::new();
    let mut seen = HashSet::new();
    let mut in_compounds = false;
    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed == "#Compounds" {
            in_compounds = true;
            continue;
        }
        if !in_compounds || trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 2 {
            return Err(format!("malformed compound row {}", line_no + 1).into());
        }
        let glyphs = parts[0].trim().to_string();
        let reading = normalize_phrase_reading(parts[1]);
        if reading.split(' ').count() != 2 || glyphs.chars().count() < 2 {
            return Err(format!("invalid two-word compound row {}", line_no + 1).into());
        }
        if !seen.insert(reading.clone()) {
            return Err(format!("duplicate compound reading {reading}").into());
        }
        phrases.push(PhraseEntry { reading, glyphs, system_rank: phrases.len() as u32 });
    }
    if phrases.len() != 409 { return Err(format!("expected 409 compound phrases, found {}", phrases.len()).into()); }
    Ok(phrases)
}

fn serialize_phrase_v1(phrases: &[PhraseEntry], out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = out_path.parent() { fs::create_dir_all(parent)?; }
    let mut file = File::create(out_path)?;
    file.write_all(b"HNPH")?;
    file.write_all(&[0x01, 0, 0, 0])?;
    file.write_all(&(phrases.len() as u32).to_le_bytes())?;
    for entry in phrases {
        let reading = entry.reading.as_bytes();
        let glyphs = entry.glyphs.as_bytes();
        if reading.len() > u16::MAX as usize || glyphs.len() > u16::MAX as usize { return Err("phrase too long".into()); }
        file.write_all(&(reading.len() as u16).to_le_bytes())?;
        file.write_all(reading)?;
        file.write_all(&(glyphs.len() as u16).to_le_bytes())?;
        file.write_all(glyphs)?;
        file.write_all(&entry.system_rank.to_le_bytes())?;
    }
    file.flush()?;
    Ok(())
}

fn serialize_v1(
    dict: &HashMap<String, Vec<char>>,
    out_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(out_path)?;

    // Magic: "HNOM"
    file.write_all(b"HNOM")?;
    // Version: 0x01
    file.write_all(&[0x01])?;
    // Reserved: 3 bytes
    file.write_all(&[0x00, 0x00, 0x00])?;

    // Entry count N
    let count = dict.len() as u32;
    file.write_all(&count.to_le_bytes())?;

    let mut ordered: Vec<_> = dict.iter().collect();
    ordered.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (reading, candidates) in ordered {
        let reading_bytes = reading.as_bytes();
        let r_len = reading_bytes.len() as u8;
        file.write_all(&[r_len])?;
        file.write_all(reading_bytes)?;

        let c_len = candidates.len() as u16;
        file.write_all(&c_len.to_le_bytes())?;

        for &ch in candidates {
            let cp = ch as u32;
            file.write_all(&cp.to_le_bytes())?;
        }
    }

    file.flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Building Hán Nôm Dictionary (v10) ===");

    let mut output_dir = std::env::args().skip(1);
    let output_dir = match (output_dir.next().as_deref(), output_dir.next()) {
        (None, None) => Path::new("hc_core/data").to_path_buf(),
        (Some("--output-dir"), Some(dir)) => Path::new(&dir).to_path_buf(),
        _ => return Err("usage: build_nom_dict.rs [--output-dir DIR]".into()),
    };

    // 1. Unihan
    let unihan_path = Path::new("data/Unihan_Readings.txt");
    let unihan_map = parse_unihan(unihan_path)?;
    println!("Unihan loaded: {} unique readings", unihan_map.len());

    // 2. NomStd
    let nomstd_path = Path::new("data/NomStandardization.csv");
    let (nomstd_rows, skipped_empty) = parse_nom_standardization(nomstd_path)?;
    println!(
        "NomStd loaded: {} valid rows (skipped {} empty CHUNOM rows)",
        nomstd_rows.len(),
        skipped_empty
    );

    // 3. cake_gao
    let cake_path = Path::new("data/cake_gao_chunom.chars.dict.yaml");
    let cake_map = parse_rime_dict(cake_path)?;
    println!("cake_gao loaded: {} unique readings", cake_map.len());

    // 4. pearapple123
    let pear_path = Path::new("data/chu_nom.dict.yaml");
    let pear_map = parse_rime_dict(pear_path)?;
    let phrases = parse_rime_phrases(pear_path)?;
    println!("pearapple123 loaded: {} unique readings", pear_map.len());
    println!("pearapple123 compounds loaded: {} two-word phrases", phrases.len());

    // Merge into combined dict
    let mut combined: HashMap<String, Vec<char>> = HashMap::new();
    let mut unique_chars: HashSet<char> = HashSet::new();
    let mut ext_b_plus_count = 0;

    // Layer 1: Unihan (authoritative Hán-Việt)
    for (reading, chars) in unihan_map {
        let entry = combined.entry(reading).or_default();
        for ch in chars {
            if !entry.contains(&ch) {
                entry.push(ch);
                unique_chars.insert(ch);
                if (ch as u32) >= 0x20000 {
                    ext_b_plus_count += 1;
                }
            }
        }
    }

    // Layer 2: cake_gao (primary Nôm)
    for (reading, chars) in cake_map {
        let entry = combined.entry(reading).or_default();
        for ch in chars {
            if !entry.contains(&ch) {
                entry.push(ch);
                unique_chars.insert(ch);
                if (ch as u32) >= 0x20000 {
                    ext_b_plus_count += 1;
                }
            }
        }
    }

    // Layer 3: pearapple123 (common Nôm supplement)
    for (reading, chars) in pear_map {
        let entry = combined.entry(reading).or_default();
        for ch in chars {
            if !entry.contains(&ch) {
                entry.push(ch);
                unique_chars.insert(ch);
                if (ch as u32) >= 0x20000 {
                    ext_b_plus_count += 1;
                }
            }
        }
    }

    // Layer 4: NomStd additions
    for row in nomstd_rows {
        let entry = combined.entry(row.reading).or_default();
        for ch in row.chunom.chars() {
            if ch != '？' && !entry.contains(&ch) {
                entry.push(ch);
                unique_chars.insert(ch);
                if (ch as u32) >= 0x20000 {
                    ext_b_plus_count += 1;
                }
            }
        }
    }

    println!("\n=== Combined Stats ===");
    println!("Total unique readings: {}", combined.len());
    println!("Total unique characters: {}", unique_chars.len());
    println!("Extension B+ characters (Nôm): {}", ext_b_plus_count);

    // Quality assertions
    assert!(
        combined.get("thiên").map_or(false, |v| v.contains(&'天')),
        "Quality assertion failed: 'thiên' must contain '天'"
    );
    assert!(
        combined.get("địa").map_or(false, |v| v.contains(&'地')),
        "Quality assertion failed: 'địa' must contain '地'"
    );
    assert!(
        combined.get("nhân").map_or(false, |v| v.contains(&'人')),
        "Quality assertion failed: 'nhân' must contain '人'"
    );
    assert!(
        combined.get("việt").map_or(false, |v| v.contains(&'越')),
        "Quality assertion failed: 'việt' must contain '越'"
    );
    assert!(
        combined.len() >= 3000,
        "Quality assertion failed: total unique readings must be >= 3000"
    );

    // Serialize to hc_core/data/han_nom_dict.bin
    let bin_path = output_dir.join("han_nom_dict.bin");
    serialize_v1(&combined, &bin_path)?;
    println!(
        "Serialized binary dictionary to {:?} (size: {} bytes)",
        bin_path,
        fs::metadata(&bin_path)?.len()
    );
    let phrase_path = output_dir.join("han_nom_phrase_dict.bin");
    serialize_phrase_v1(&phrases, &phrase_path)?;
    println!("Serialized phrase dictionary to {:?} ({} entries)", phrase_path, phrases.len());

    // Generate quality report
    let mut report = File::create(output_dir.join("quality_report.txt"))?;
    writeln!(report, "Hán Nôm Dictionary Quality Report")?;
    writeln!(report, "=================================")?;
    writeln!(report, "Total unique readings: {}", combined.len())?;
    writeln!(report, "Total unique characters: {}", unique_chars.len())?;
    writeln!(
        report,
        "Extension B+ (Nôm) characters: {}",
        ext_b_plus_count
    )?;
    writeln!(
        report,
        "Skipped empty CHUNOM rows from NomStd: {}",
        skipped_empty
    )?;
    writeln!(report, "\nSample assertions verified:")?;
    writeln!(report, "  thiên -> 天: PASS")?;
    writeln!(report, "  địa -> 地: PASS")?;
    writeln!(report, "  nhân -> 人: PASS")?;
    writeln!(report, "  việt -> 越: PASS")?;
    println!("Generated quality_report.txt");

    Ok(())
}
