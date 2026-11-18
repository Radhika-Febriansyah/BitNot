//! Text folding for search and duplicate detection.
//!
//! Folding makes matching insensitive to case, accents, and a few common
//! character variants (CLAUDE.md §14). It is still exact matching on the folded
//! text: there is no typo correction. Stored metadata is never changed.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

const SEPARATOR: char = '\u{1f}';

/// Lower-cases, removes accents, unifies full-width forms and ligatures,
/// and maps katakana to hiragana.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.nfkd().filter(|c| !is_combining_mark(*c)) {
        match ch {
            'ß' | 'ẞ' => out.push_str("ss"),
            'æ' | 'Æ' => out.push_str("ae"),
            'œ' | 'Œ' => out.push_str("oe"),
            'ø' | 'Ø' => out.push('o'),
            'đ' | 'Đ' | 'ð' | 'Ð' => out.push('d'),
            'ł' | 'Ł' => out.push('l'),
            'ı' => out.push('i'),
            'þ' | 'Þ' => out.push_str("th"),
            // Katakana to hiragana, so either script finds the other.
            '\u{30A1}'..='\u{30F6}' => {
                out.push(char::from_u32(ch as u32 - 0x60).unwrap_or(ch));
            }
            SEPARATOR => out.push(' '),
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out
}

/// One folded string holding title, artist, and album, used for fast search.
pub fn search_key(title: &str, artist: &str, album: &str) -> String {
    format!(
        "{}{SEPARATOR}{}{SEPARATOR}{}",
        fold(title),
        fold(artist),
        fold(album)
    )
}

/// Splits a search key back into its folded title, artist, and album.
pub fn key_parts(key: &str) -> (&str, &str, &str) {
    let mut parts = key.splitn(3, SEPARATOR);
    (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_and_accents() {
        assert_eq!(fold("Élan VITAL"), "elan vital");
        assert_eq!(fold("Ünder Gláss"), "under glass");
        assert_eq!(fold("Beyoncé"), fold("beyonce"));
        assert_eq!(fold("Sigur Rós"), "sigur ros");
    }

    #[test]
    fn folds_common_character_variants() {
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Mø"), "mo");
        assert_eq!(fold("Ægir Œuvre"), "aegir oeuvre");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("ＡＢＣ　１２３"), "abc 123", "full-width forms");
    }

    #[test]
    fn unifies_kana_and_keeps_other_scripts() {
        assert_eq!(fold("カタカナ"), fold("かたかな"));
        assert_eq!(fold("東京"), "東京");
        assert_eq!(fold("한국"), fold("한국"));
        assert_eq!(fold("Привет"), "привет");
    }

    #[test]
    fn does_not_guess_at_typos() {
        assert_ne!(fold("harbour"), fold("harbor"));
        assert_ne!(fold("nihgt"), fold("night"));
    }

    #[test]
    fn search_key_round_trips_parts() {
        let key = search_key("Night Bus", "Mira Solén", "Low Tide");
        assert_eq!(key_parts(&key), ("night bus", "mira solen", "low tide"));
    }
}
