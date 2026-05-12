use crate::audio_discovery::{discover_audio_files, AudioFile};
use anyhow::Result;
use regex::Regex;
use std::cmp::Ordering;
use std::path::Path;
use std::sync::OnceLock;

/// Gestiona asignacion de indices globales para nombres legacy.
///
/// Reglas:
/// - base index = max prefijo existente en USB + 1
/// - si USB no tiene prefijos validos, inicia en 1
/// - prefijo siempre con padding minimo de 4 digitos
#[derive(Debug, Clone)]
pub struct IndexManager {
    next_index: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PrefixWidthReport {
    pub has_legacy_3_digits: bool,
    pub has_modern_4_digits: bool,
}

impl PrefixWidthReport {
    pub fn is_mixed_transition(self) -> bool {
        self.has_legacy_3_digits && self.has_modern_4_digits
    }
}


impl IndexManager {
    pub fn from_max_existing_index(max_existing_index: usize) -> Self {
        Self {
            next_index: max_existing_index.saturating_add(1).max(1),
        }
    }

    pub fn from_usb_scan(usb_mount: &Path) -> Result<Self> {
        let report = discover_audio_files(usb_mount)?;
        let max_existing_index = report
            .audio_files
            .iter()
            .filter_map(|file| parse_global_prefix_index(&file.filename))
            .max()
            .unwrap_or(0);

        Ok(Self::from_max_existing_index(max_existing_index))
    }

    pub fn detect_prefix_width_transition(usb_mount: &Path) -> Result<PrefixWidthReport> {
        let report = discover_audio_files(usb_mount)?;
        let mut width_report = PrefixWidthReport::default();

        for file in &report.audio_files {
            if let Some(prefix_digits) = parse_prefix_digits(&file.filename) {
                match prefix_digits.len() {
                    3 => width_report.has_legacy_3_digits = true,
                    4 => width_report.has_modern_4_digits = true,
                    _ => {}
                }
            }
        }

        Ok(width_report)
    }

    pub fn peek_next_index(&self) -> usize {
        self.next_index
    }

    pub fn allocate_next(&mut self) -> usize {
        let current = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        current
    }

    pub fn format_padded_prefix(index: usize) -> String {
        format!("{:04}", index)
    }

    pub fn sort_new_files_natural(files: &mut [AudioFile]) {
        files.sort_by(natural_audio_file_cmp);
    }
}

fn prefix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d{3,4})_").expect("valid 3-or-4 prefix regex"))
}

fn parse_prefix_digits(file_name: &str) -> Option<&str> {
    let captures = prefix_re().captures(file_name)?;
    captures.get(1).map(|m| m.as_str())
}

fn parse_global_prefix_index(file_name: &str) -> Option<usize> {
    let prefix = parse_prefix_digits(file_name)?;
    prefix.parse::<usize>().ok()
}

fn natural_audio_file_cmp(a: &AudioFile, b: &AudioFile) -> Ordering {
    natural_cmp_case_insensitive(&a.filename, &b.filename)
        .then_with(|| a.path.to_string_lossy().cmp(&b.path.to_string_lossy()))
}

fn natural_cmp_case_insensitive(left: &str, right: &str) -> Ordering {
    let mut l_iter = left.chars().peekable();
    let mut r_iter = right.chars().peekable();

    loop {
        match (l_iter.peek().copied(), r_iter.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(lc), Some(rc)) => {
                if lc.is_ascii_digit() && rc.is_ascii_digit() {
                    let l_num = consume_number(&mut l_iter);
                    let r_num = consume_number(&mut r_iter);
                    match cmp_number_strings(&l_num, &r_num) {
                        Ordering::Equal => continue,
                        non_eq => return non_eq,
                    }
                } else {
                    let l = l_iter.next().expect("peeked char must exist").to_ascii_lowercase();
                    let r = r_iter.next().expect("peeked char must exist").to_ascii_lowercase();
                    match l.cmp(&r) {
                        Ordering::Equal => continue,
                        non_eq => return non_eq,
                    }
                }
            }
        }
    }
}

fn consume_number(iter: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut out = String::new();
    while let Some(ch) = iter.peek().copied() {
        if !ch.is_ascii_digit() {
            break;
        }
        out.push(ch);
        iter.next();
    }
    out
}

fn cmp_number_strings(left: &str, right: &str) -> Ordering {
    let left_trimmed = left.trim_start_matches('0');
    let right_trimmed = right.trim_start_matches('0');
    let left_norm = if left_trimmed.is_empty() { "0" } else { left_trimmed };
    let right_norm = if right_trimmed.is_empty() { "0" } else { right_trimmed };

    left_norm
        .len()
        .cmp(&right_norm.len())
        .then_with(|| left_norm.cmp(right_norm))
        .then_with(|| left.len().cmp(&right.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_index_manager_base_index_from_max() {
        let mgr = IndexManager::from_max_existing_index(183);
        assert_eq!(mgr.peek_next_index(), 184);
    }

    #[test]
    fn test_index_manager_allocates_sequential_indexes() {
        let mut mgr = IndexManager::from_max_existing_index(2);
        assert_eq!(mgr.allocate_next(), 3);
        assert_eq!(mgr.allocate_next(), 4);
        assert_eq!(mgr.peek_next_index(), 5);
    }

    #[test]
    fn test_index_manager_prefix_padding() {
        assert_eq!(IndexManager::format_padded_prefix(1), "0001");
        assert_eq!(IndexManager::format_padded_prefix(48), "0048");
        assert_eq!(IndexManager::format_padded_prefix(150), "0150");
        assert_eq!(IndexManager::format_padded_prefix(1250), "1250");
    }

    #[test]
    fn test_index_manager_usb_scan_uses_highest_prefix() -> Result<()> {
        let usb = TempDir::new()?;
        fs::create_dir_all(usb.path().join("VOL_01"))?;
        fs::create_dir_all(usb.path().join("VOL_04"))?;
        fs::write(
            usb.path().join("VOL_01/002_alpha___________11111111.mp3"),
            b"a",
        )?;
        fs::write(
            usb.path().join("VOL_04/1183_beta___________22222222.mp3"),
            b"b",
        )?;

        let mgr = IndexManager::from_usb_scan(usb.path())?;
        assert_eq!(mgr.peek_next_index(), 1184);
        Ok(())
    }

    #[test]
    fn test_detect_prefix_width_transition_reports_mixed_usb() -> Result<()> {
        let usb = TempDir::new()?;
        fs::create_dir_all(usb.path().join("VOL_01"))?;
        fs::write(
            usb.path().join("VOL_01/999_old_____________aaaaaaaa.mp3"),
            b"a",
        )?;
        fs::write(
            usb.path().join("VOL_01/1000_new____________bbbbbbbb.mp3"),
            b"b",
        )?;

        let report = IndexManager::detect_prefix_width_transition(usb.path())?;
        assert!(report.has_legacy_3_digits);
        assert!(report.has_modern_4_digits);
        assert!(report.is_mixed_transition());
        Ok(())
    }

    #[test]
    fn test_natural_sort_orders_numbers_correctly() {
        let mut files = vec![
            AudioFile {
                path: PathBuf::from("/tmp/10.mp3"),
                filename: "10.mp3".to_string(),
                extension: "mp3".to_string(),
                size_bytes: 1,
                depth: 0,
            },
            AudioFile {
                path: PathBuf::from("/tmp/2.mp3"),
                filename: "2.mp3".to_string(),
                extension: "mp3".to_string(),
                size_bytes: 1,
                depth: 0,
            },
            AudioFile {
                path: PathBuf::from("/tmp/A11.mp3"),
                filename: "A11.mp3".to_string(),
                extension: "mp3".to_string(),
                size_bytes: 1,
                depth: 0,
            },
            AudioFile {
                path: PathBuf::from("/tmp/a2.mp3"),
                filename: "a2.mp3".to_string(),
                extension: "mp3".to_string(),
                size_bytes: 1,
                depth: 0,
            },
        ];

        IndexManager::sort_new_files_natural(&mut files);
        let sorted: Vec<String> = files.into_iter().map(|f| f.filename).collect();
        assert_eq!(sorted, vec!["2.mp3", "10.mp3", "a2.mp3", "A11.mp3"]);
    }
}
