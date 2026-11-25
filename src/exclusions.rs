use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

use crate::properties::{TextEncoding, TextFile};

const STACK_PATH_LEN: usize = 512;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlUpcaseUnicodeChar(source_character: u16) -> u16;
}

fn ntfs_upcase_unit(unit: u16) -> u16 {
    let normalized = if unit == b'/' as u16 {
        b'\\' as u16
    } else {
        unit
    };
    unsafe { RtlUpcaseUnicodeChar(normalized) }
}

pub fn ntfs_upcase_utf16_slice(units: &[u16], out: &mut Vec<u16>) {
    out.clear();
    out.reserve(units.len());
    for &unit in units {
        out.push(ntfs_upcase_unit(unit));
    }
}

pub fn ntfs_upcase_utf16(path: &str) -> Box<[u16]> {
    let mut out = Vec::new();
    for unit in path.encode_utf16() {
        out.push(ntfs_upcase_unit(unit));
    }
    out.into_boxed_slice()
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFullPathNameW(
        file_name: *const u16,
        buffer_length: u32,
        buffer: *mut u16,
        file_part: *mut *mut u16,
    ) -> u32;
}

/// As .NET Path.IsPathFullyQualified: a UNC or device path (`\\`), or a drive with its root
/// (`C:\`). Not a relative one (e.g. `game.exe`, `C:game.exe`, `\Games\game.exe`).
fn is_fully_qualified(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with("\\\\") || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\')
}

/// The full path as Windows writes the path of a process (GetFullPathName: "." and ".." resolved,
/// the dots and spaces at the end of a name dropped), as .NET Path.GetFullPath; None if it fails
fn full_path(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = vec![0u16; 512];
    loop {
        let len = unsafe {
            GetFullPathNameW(wide.as_ptr(), buf.len() as u32, buf.as_mut_ptr(), std::ptr::null_mut())
        } as usize;
        if len == 0 {
            return None;
        }
        if len < buf.len() {
            return String::from_utf16(&buf[..len]).ok();
        }
        // Too small: len is the size needed (with the terminating null)
        buf.resize(len, 0);
    }
}

/// A line of the exclusions (or a path added) as it is kept and compared, as .NET
/// (Ctx.Exclusion.SetPrograms): the spaces and the quotes around it dropped (e.g. pasted from
/// "Copy as path" of Explorer: no path can have a quote), and a full path written as Windows gives
/// the path of a process ("/" as "\", ".." resolved...). A relative one (e.g. written in the file by
/// hand) is kept as it is: it would be taken from the current folder. None if empty.
pub fn normalize_exclusion_entry(line: &str) -> Option<String> {
    let unquoted = line.trim().trim_matches('"').trim();
    if unquoted.is_empty() {
        return None;
    }
    let path = unquoted.replace('/', "\\");
    if is_fully_qualified(&path) {
        Some(full_path(&path).unwrap_or(path))
    } else {
        Some(path)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExclusionMatcher {
    entries: HashSet<Box<[u16]>>,
}

impl ExclusionMatcher {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn matches_utf16(&self, units: &[u16]) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        if units.len() <= STACK_PATH_LEN {
            let mut stack = [0u16; STACK_PATH_LEN];
            for (dst, &src) in stack.iter_mut().zip(units.iter()) {
                *dst = ntfs_upcase_unit(src);
            }
            return self.entries.contains(&stack[..units.len()]);
        }
        let mut heap = Vec::new();
        ntfs_upcase_utf16_slice(units, &mut heap);
        self.entries.contains(heap.as_slice())
    }

    pub fn matches_path(&self, path: &str) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        let mut stack = [0u16; STACK_PATH_LEN];
        let mut len = 0usize;
        let mut heap = Vec::new();
        for unit in path.encode_utf16() {
            let up = ntfs_upcase_unit(unit);
            if heap.is_empty() {
                if len < STACK_PATH_LEN {
                    stack[len] = up;
                    len += 1;
                } else {
                    heap.reserve(STACK_PATH_LEN * 2);
                    heap.extend_from_slice(&stack);
                    heap.push(up);
                }
            } else {
                heap.push(up);
            }
        }
        if heap.is_empty() {
            self.entries.contains(&stack[..len])
        } else {
            self.entries.contains(heap.as_slice())
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExclusionList {
    paths: Vec<String>,
    keys: HashSet<Box<[u16]>>,
}

impl ExclusionList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_lines<I, S>(lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut list = Self::new();
        for line in lines {
            list.add(line.as_ref());
        }
        list
    }

    pub fn entries(&self) -> &[String] {
        &self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn contains(&self, path: &str) -> bool {
        let Some(norm) = normalize_exclusion_entry(path) else {
            return false;
        };
        let key = ntfs_upcase_utf16(&norm);
        self.keys.contains(key.as_ref())
    }

    pub fn add(&mut self, path: &str) -> bool {
        let Some(norm) = normalize_exclusion_entry(path) else {
            return false;
        };
        let key = ntfs_upcase_utf16(&norm);
        if self.keys.insert(key) {
            self.paths.push(norm);
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, path: &str) -> bool {
        let Some(norm) = normalize_exclusion_entry(path) else {
            return false;
        };
        let key = ntfs_upcase_utf16(&norm);
        if self.keys.remove(key.as_ref()) {
            if let Some(pos) = self
                .paths
                .iter()
                .position(|existing| ntfs_upcase_utf16(existing) == key)
            {
                self.paths.remove(pos);
            }
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.keys.clear();
    }

    pub fn to_matcher(&self) -> ExclusionMatcher {
        ExclusionMatcher {
            entries: self.keys.clone(),
        }
    }
}

pub struct ExclusionsFile;

impl ExclusionsFile {
    pub fn read(path: &Path) -> io::Result<ExclusionList> {
        match TextFile::read(path) {
            Ok(file) => Ok(ExclusionList::from_lines(file.lines)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(ExclusionList::new()),
            Err(err) => Err(err),
        }
    }

    pub fn write(path: &Path, list: &ExclusionList) -> io::Result<()> {
        if list.is_empty() {
            match fs::symlink_metadata(path) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(err) => return Err(err),
                Ok(meta) if meta.file_type().is_symlink() => {
                    let (encoding, newline) = read_format_or_default(path);
                    return TextFile::write(
                        path,
                        &TextFile {
                            lines: Vec::new(),
                            newline,
                            encoding,
                        },
                    );
                }
                Ok(meta) => {
                    // Windows' own error (in its language), as for the properties file
                    if meta.is_dir() || meta.permissions().readonly() {
                        return Err(io::Error::from_raw_os_error(
                            crate::properties::ERROR_ACCESS_DENIED,
                        ));
                    }
                    return fs::remove_file(path);
                }
            }
        }

        let (encoding, newline) = read_format_or_default(path);
        TextFile::write(
            path,
            &TextFile {
                lines: list.entries().to_vec(),
                newline,
                encoding,
            },
        )
    }
}

fn read_format_or_default(path: &Path) -> (TextEncoding, String) {
    match TextFile::read(path) {
        Ok(existing) => (existing.encoding, existing.newline),
        Err(_) => (TextEncoding::Utf8 { bom: false }, "\r\n".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::os::windows::fs::symlink_file;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    static TEST_SEQ: AtomicU32 = AtomicU32::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> io::Result<Self> {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = env::temp_dir().join(format!(
                "w10wheel-excl-test-{}-{ts}-{seq}",
                std::process::id()
            ));
            fs::create_dir(&dir)?;
            Ok(Self(dir))
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn normalization_deduplication_and_ntfs_case_rules() {
        let mut list = ExclusionList::from_lines([
            "",
            "   ",
            "\" C:/Games/My#App/game.exe \"",
            "c:\\games\\my#app\\GAME.EXE",
            "//server/share/Tools/app.exe",
            "C:\\Users\\straße.exe",
        ]);

        assert_eq!(
            list.entries(),
            &[
                "C:\\Games\\My#App\\game.exe",
                "\\\\server\\share\\Tools\\app.exe",
                "C:\\Users\\straße.exe",
            ]
        );
        assert!(list.contains("c:\\games\\my#app\\game.exe"));
        assert!(list.contains("\\\\SERVER\\SHARE\\tools\\APP.EXE"));

        // NTFS uppercase maps ä -> Ä via RtlUpcaseUnicodeChar while keeping ß distinct from ss/SS.
        assert!(list.add("C:\\Users\\ärger.exe"));
        assert!(!list.add("c:\\users\\ÄRGER.EXE"));
        assert!(list.contains("c:\\users\\Ärger.exe"));

        let matcher = list.to_matcher();
        assert!(matcher.matches_path("C:/USERS/ÄRGER.EXE"));
        assert!(matcher.matches_path("C:\\Users\\straße.exe"));
        assert!(!matcher.matches_path("C:\\Users\\strasse.exe"));
        assert!(!matcher.matches_path("C:\\Users\\STRASSE.EXE"));

        let wide: Vec<u16> = "C:/Games/My#App/GAME.EXE".encode_utf16().collect();
        assert!(matcher.matches_utf16(&wide));

        assert!(list.remove("\"c:\\games\\my#app\\game.exe\""));
        assert!(!list.contains("C:\\Games\\My#App\\game.exe"));
    }

    #[test]
    fn empty_matcher_always_false_and_long_paths_work() {
        let empty = ExclusionMatcher::default();
        assert!(empty.is_empty());
        assert!(!empty.matches_path("C:\\Windows\\notepad.exe"));
        assert!(!empty.matches_utf16(&[b'C' as u16, b':' as u16]));

        let long_dir = "a\\".repeat(300);
        let long_path = format!("C:\\{long_dir}long_app.exe");
        let list = ExclusionList::from_lines([&long_path]);
        let matcher = list.to_matcher();
        let wide_upper: Vec<u16> = long_path.to_uppercase().encode_utf16().collect();
        assert!(matcher.matches_utf16(&wide_upper));
        assert!(matcher.matches_path(&long_path.replace('\\', "/").to_uppercase()));
    }

    #[test]
    fn exclusions_file_read_write_preserves_encoding_and_handles_empty_and_readonly() -> io::Result<()>
    {
        let dir = TestDir::new()?;
        let path = dir.0.join(".W10Wheel.exclusions");

        // Missing file reads as empty and writing empty does not create a file.
        assert!(ExclusionsFile::read(&path)?.is_empty());
        ExclusionsFile::write(&path, &ExclusionList::new())?;
        assert!(!path.exists());

        // Write UTF-16 LE BOM + LF file, read it back, and re-write preserving format.
        let initial = TextFile {
            lines: vec![
                "C:\\Program Files\\App\\app.exe".to_owned(),
                "C:\\Tools\\tool.exe".to_owned(),
            ],
            newline: "\n".to_owned(),
            encoding: TextEncoding::Utf16Le,
        };
        TextFile::write(&path, &initial)?;

        let mut list = ExclusionsFile::read(&path)?;
        assert_eq!(list.len(), 2);
        list.remove("c:\\tools\\TOOL.EXE");
        ExclusionsFile::write(&path, &list)?;

        let reread_raw = TextFile::read(&path)?;
        assert_eq!(reread_raw.encoding, TextEncoding::Utf16Le);
        assert_eq!(reread_raw.newline, "\n");
        assert_eq!(reread_raw.lines, vec!["C:\\Program Files\\App\\app.exe"]);

        // Read-only file rejects modification and deletion.
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms.clone())?;
        assert_eq!(
            ExclusionsFile::write(&path, &ExclusionList::new())
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(&path, perms)?;

        // Writing empty list removes a regular file.
        ExclusionsFile::write(&path, &ExclusionList::new())?;
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn writing_empty_exclusions_through_symlink_clears_target_without_deleting_link()
    -> io::Result<()> {
        let dir = TestDir::new()?;
        let target = dir.0.join("shared.exclusions");
        let link = dir.0.join(".W10Wheel.exclusions");
        TextFile::write(
            &target,
            &TextFile {
                lines: vec!["C:\\Apps\\one.exe".to_owned()],
                newline: "\n".to_owned(),
                encoding: TextEncoding::Utf8 { bom: true },
            },
        )?;
        if symlink_file(&target, &link).is_err() {
            // Skip if privilege to create file symlinks is absent on this host.
            return Ok(());
        }

        ExclusionsFile::write(&link, &ExclusionList::new())?;
        assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
        // Emptied, its encoding kept: the BOM alone (no line, so no newline to keep)
        assert_eq!(fs::read(&target)?, [0xEF, 0xBB, 0xBF]);
        let target_raw = TextFile::read(&target)?;
        assert!(target_raw.lines.is_empty());
        assert_eq!(target_raw.encoding, TextEncoding::Utf8 { bom: true });
        Ok(())
    }

    #[test]
    fn full_paths_are_normalized_as_windows_writes_process_paths() {
        // Quotes trimmed at either end (as .NET Trim('"')), then the spaces inside them
        assert_eq!(
            normalize_exclusion_entry("  \" C:\\Games\\game.exe \"  ").as_deref(),
            Some("C:\\Games\\game.exe")
        );
        assert_eq!(
            normalize_exclusion_entry("\"C:\\Games\\game.exe").as_deref(),
            Some("C:\\Games\\game.exe")
        );
        // "/" ".", ".." and trailing dots resolved for a full path
        assert_eq!(
            normalize_exclusion_entry("C:/Games/./old/../game.exe.").as_deref(),
            Some("C:\\Games\\game.exe")
        );
        assert_eq!(
            normalize_exclusion_entry("\\\\server\\share\\x\\..\\app.exe").as_deref(),
            Some("\\\\server\\share\\app.exe")
        );
        // A relative one is kept as written (not taken from the current folder)
        assert_eq!(normalize_exclusion_entry("game.exe").as_deref(), Some("game.exe"));
        assert_eq!(
            normalize_exclusion_entry("..\\game.exe").as_deref(),
            Some("..\\game.exe")
        );
        assert_eq!(normalize_exclusion_entry(" \"\" "), None);

        // Matched as the process path Windows gives
        let list = ExclusionList::from_lines(["C:/Games/old/../Game.EXE"]);
        assert!(list.to_matcher().matches_path("c:\\games\\game.exe"));
    }
}
