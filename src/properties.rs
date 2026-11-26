use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use crate::config::{Config, CustomTableState, UiLanguage};
use crate::exclusions::{ExclusionList, ExclusionsFile};

pub const PROGRAM_NAME: &str = "W10Wheel";
pub const DEFAULT_PROFILE: &str = "Default";

const PROP_PREFIX: &str = ".W10Wheel";
const PROP_EXT: &str = "properties";
const EXCLUSIONS_EXT: &str = "exclusions";
const INVALID_FILE_CHARS: &str = "\"<>|:*?\\/";
const REPLACE_RETRY_COUNT: usize = 3;
const REPLACE_RETRY_DELAY: Duration = Duration::from_millis(10);
pub(crate) const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_CANT_RESOLVE_FILENAME: i32 = 1921;

static TEMP_SEQUENCE: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8 { bom: bool },
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFile {
    pub lines: Vec<String>,
    pub newline: String,
    pub encoding: TextEncoding,
}

impl TextFile {
    pub fn read(path: &Path) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        Ok(Self::from_bytes(&bytes))
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let (encoding, text) = decode_bytes(bytes);
        let newline = detect_newline(&text).to_owned();
        let lines = split_lines(&text);
        Self {
            lines,
            newline,
            encoding,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(line);
            out.push_str(&self.newline);
        }
        encode_text(&out, self.encoding)
    }

    /// Writes atomically via a temporary file in the target directory, following symlinks and
    /// preserving read-only rejection.
    pub fn write(path: &Path, file: &Self) -> io::Result<()> {
        let target = resolve_write_target(path)?;
        if let Ok(meta) = fs::metadata(&target) {
            // Windows' own errors (in its language, as a direct write would get them)
            if meta.is_dir() || meta.permissions().readonly() {
                return Err(io::Error::from_raw_os_error(ERROR_ACCESS_DENIED));
            }
        }

        let directory = target.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Target path has no parent.")
        })?;
        fs::create_dir_all(directory)?;

        let payload = file.to_bytes();
        let existed = target.exists();
        let (temporary, mut handle) = create_temp_file(directory, &target)?;
        let write_result = handle.write_all(&payload).and_then(|()| handle.sync_all());
        drop(handle);
        // The temporary file could not be written (e.g. a full disk): the file is left as it was
        if let Err(err) = write_result {
            let _ = fs::remove_file(&temporary);
            return Err(err);
        }

        let replaced = replace_with_retry(&temporary, &target, existed);
        // Not left behind, also if ReplaceFile failed midway
        let _ = fs::remove_file(&temporary);
        match replaced {
            Ok(()) => Ok(()),
            // The replacement failed (e.g. the file is open in a program that does not share its
            // deletion): written directly, as .NET does. The temporary file has been deleted, which
            // gives its room back first.
            Err(err) if existed => write_direct(&target, &payload).map_err(|_| err),
            Err(err) => Err(err),
        }
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn ReplaceFileW(
        replaced: *const u16,
        replacement: *const u16,
        backup: *const u16,
        flags: u32,
        exclude: *mut std::ffi::c_void,
        reserved: *mut std::ffi::c_void,
    ) -> i32;
}

const REPLACEFILE_IGNORE_MERGE_ERRORS: u32 = 0x0000_0002;

/// A path of this length or longer, in UTF-16 units with its NUL, is given the `\\?\` prefix, as
/// std does for its own calls (MAX_PATH less the 12 units that a directory must leave for a 8.3
/// file name)
const LEGACY_MAX_PATH: usize = 248;

/// The path, NUL-terminated, for a Win32 function called directly (std's own functions do this
/// themselves): one long once absolute is given the `\\?\` prefix, as std and .NET (File.Replace)
/// do. Without it, such a function fails for a long path (ERROR_PATH_NOT_FOUND) unless long
/// paths are enabled for both the system and the process. See long_path.
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let mut wide = long_path(path.as_os_str().encode_wide().collect(), |wide| {
        // GetFullPathNameW, as std's get_long_path
        std::path::absolute(std::ffi::OsString::from_wide(wide))
            .ok()
            .map(|absolute| absolute.as_os_str().encode_wide().collect())
    });
    wide.push(0);
    wide
}

/// The path (no NUL) as std makes it for its own calls (sys::path::windows::get_long_path, with
/// prefer_verbatim false), `absolute` being GetFullPathNameW (None if it fails). Passed as it is
/// if already verbatim (`\\?\`, `\??\`) or empty, or short and absolute (`C:\`, `C:`, `\\`). Else
/// made absolute (normalized: `..` and `/` resolved, a relative one with the current directory),
/// and, if that is long, given the prefix: `\\?\C:\`, `\\?\` for `\\.\` (a device path, which a
/// file path may also be), `\\?\UNC\` for a share. A long path whose normalized form is short is
/// passed so, with no prefix. One that cannot be made absolute is passed as it is (not
/// normalized, `\\?\` would take it literally): the call then fails as it would have.
fn long_path(path: Vec<u16>, absolute: impl FnOnce(&[u16]) -> Option<Vec<u16>>) -> Vec<u16> {
    const SEP: u16 = b'\\' as u16;
    const ALT_SEP: u16 = b'/' as u16;
    const QUESTION: u16 = b'?' as u16;
    const DOT: u16 = b'.' as u16;
    const COLON: u16 = b':' as u16;
    let prefixed = |prefix: &str, rest: &[u16]| -> Vec<u16> {
        prefix.encode_utf16().chain(rest.iter().copied()).collect()
    };
    let short = path.len() + 1 < LEGACY_MAX_PATH;
    let as_it_is = match path.as_slice() {
        [] | [SEP, SEP, QUESTION, SEP, ..] | [SEP, QUESTION, QUESTION, SEP, ..] => true,
        [drive, COLON] | [drive, COLON, SEP | ALT_SEP, ..] => {
            short && *drive != SEP && *drive != ALT_SEP
        }
        [SEP | ALT_SEP, SEP | ALT_SEP, ..] => short,
        _ => false,
    };
    if as_it_is {
        return path;
    }
    let Some(absolute) = absolute(&path) else {
        return path;
    };
    if absolute.len() + 1 < LEGACY_MAX_PATH {
        return absolute;
    }
    match absolute.as_slice() {
        [_, COLON, SEP, ..] => prefixed(r"\\?\", &absolute),
        [SEP, SEP, DOT, SEP, rest @ ..] => prefixed(r"\\?\", rest),
        [SEP, SEP, QUESTION, SEP, ..] | [SEP, QUESTION, QUESTION, SEP, ..] => absolute,
        [SEP, SEP, rest @ ..] => prefixed(r"\\?\UNC\", rest),
        _ => absolute,
    }
}

/// The existing file replaced by the temporary one with ReplaceFile, as .NET File.Replace does: its
/// attributes (e.g. hidden, which a new file would lose) and its permissions are kept; if they
/// cannot be merged (e.g. a file system without them) it is still replaced. A new file is renamed.
fn replace_file(temporary: &Path, target: &Path, existed: bool) -> io::Result<()> {
    if !existed {
        return fs::rename(temporary, target);
    }
    let replaced = wide_path(target);
    let replacement = wide_path(temporary);
    let ok = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_IGNORE_MERGE_ERRORS,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn replace_with_retry(temporary: &Path, target: &Path, existed: bool) -> io::Result<()> {
    for attempt in 0..REPLACE_RETRY_COUNT {
        match replace_file(temporary, target, existed) {
            Ok(()) => return Ok(()),
            Err(err) if attempt + 1 < REPLACE_RETRY_COUNT && is_transient_replace_error(&err) => {
                thread::sleep(REPLACE_RETRY_DELAY);
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("retry loop returns on last attempt")
}

/// The existing file written in place (truncated, not created again: a hidden file can be opened
/// so, and its attributes stay)
fn write_direct(target: &Path, payload: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).truncate(true).open(target)?;
    file.write_all(payload)?;
    file.sync_all()
}

fn create_temp_file(directory: &Path, target: &Path) -> io::Result<(PathBuf, fs::File)> {
    let target_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("W10Wheel");
    loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let suffix = format!("-{}-{sequence}.tmp", std::process::id());
        let max_prefix_units = 255usize.saturating_sub(suffix.len());
        let prefix = truncate_utf16_prefix(target_name, max_prefix_units);
        let temporary = directory.join(format!("{prefix}{suffix}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
}

fn truncate_utf16_prefix(input: &str, max_units: usize) -> &str {
    let mut units = 0usize;
    let mut end = 0usize;
    for (idx, ch) in input.char_indices() {
        let len = ch.len_utf16();
        if units + len > max_units {
            break;
        }
        units += len;
        end = idx + ch.len_utf8();
    }
    &input[..end]
}

fn is_transient_replace_error(err: &io::Error) -> bool {
    // Access denied, sharing or lock violations, and ReplaceFile's own failures to move a file
    // that is open (ERROR_UNABLE_TO_REMOVE_REPLACED, ERROR_UNABLE_TO_MOVE_REPLACEMENT(_2))
    err.kind() == io::ErrorKind::PermissionDenied
        || matches!(err.raw_os_error(), Some(5 | 32 | 33 | 1175 | 1176 | 1177))
}

fn resolve_write_target(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..40 {
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = fs::read_link(&current)?;
                current = if target.is_absolute() {
                    target
                } else {
                    current
                        .parent()
                        .map_or_else(|| target.clone(), |parent| parent.join(&target))
                };
            }
            Ok(_) => return Ok(current),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(current),
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::from_raw_os_error(ERROR_CANT_RESOLVE_FILENAME))
}

fn decode_bytes(bytes: &[u8]) -> (TextEncoding, String) {
    // Check longer 4-byte BOMs before 2-byte UTF-16 BOMs.
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (
            TextEncoding::Utf8 { bom: true },
            String::from_utf8_lossy(rest).into_owned(),
        );
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE, 0x00, 0x00]) {
        return (
            TextEncoding::Utf32Le,
            decode_utf32(rest, u32::from_le_bytes),
        );
    }
    if let Some(rest) = bytes.strip_prefix(&[0x00, 0x00, 0xFE, 0xFF]) {
        return (
            TextEncoding::Utf32Be,
            decode_utf32(rest, u32::from_be_bytes),
        );
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return (
            TextEncoding::Utf16Le,
            decode_utf16(rest, u16::from_le_bytes),
        );
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return (
            TextEncoding::Utf16Be,
            decode_utf16(rest, u16::from_be_bytes),
        );
    }
    (
        TextEncoding::Utf8 { bom: false },
        String::from_utf8_lossy(bytes).into_owned(),
    )
}

fn decode_utf16(bytes: &[u8], read: fn([u8; 2]) -> u16) -> String {
    let (chunks, remainder) = bytes.as_chunks::<2>();
    let mut out: String = char::decode_utf16(chunks.iter().copied().map(read))
        .map(|ch| ch.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if !remainder.is_empty() {
        out.push(char::REPLACEMENT_CHARACTER);
    }
    out
}

fn decode_utf32(bytes: &[u8], read: fn([u8; 4]) -> u32) -> String {
    let (chunks, remainder) = bytes.as_chunks::<4>();
    let mut out: String = chunks
        .iter()
        .copied()
        .map(|chunk| char::from_u32(read(chunk)).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if !remainder.is_empty() {
        out.push(char::REPLACEMENT_CHARACTER);
    }
    out
}

fn encode_text(text: &str, encoding: TextEncoding) -> Vec<u8> {
    match encoding {
        TextEncoding::Utf8 { bom: false } => text.as_bytes().to_vec(),
        TextEncoding::Utf8 { bom: true } => {
            let mut out = Vec::with_capacity(3 + text.len());
            out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            out.extend_from_slice(text.as_bytes());
            out
        }
        TextEncoding::Utf16Le => {
            let mut out = Vec::with_capacity(2 + text.len() * 2);
            out.extend_from_slice(&[0xFF, 0xFE]);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out
        }
        TextEncoding::Utf16Be => {
            let mut out = Vec::with_capacity(2 + text.len() * 2);
            out.extend_from_slice(&[0xFE, 0xFF]);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_be_bytes());
            }
            out
        }
        TextEncoding::Utf32Le => {
            let mut out = Vec::with_capacity(4 + text.len() * 4);
            out.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x00]);
            for ch in text.chars() {
                out.extend_from_slice(&(ch as u32).to_le_bytes());
            }
            out
        }
        TextEncoding::Utf32Be => {
            let mut out = Vec::with_capacity(4 + text.len() * 4);
            out.extend_from_slice(&[0x00, 0x00, 0xFE, 0xFF]);
            for ch in text.chars() {
                out.extend_from_slice(&(ch as u32).to_be_bytes());
            }
            out
        }
    }
}

fn detect_newline(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else if text.contains('\n') {
        "\n"
    } else if text.contains('\r') {
        "\r"
    } else {
        "\r\n"
    }
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                lines.push(text[start..i].to_owned());
                i += if bytes.get(i + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = i;
            }
            b'\n' => {
                lines.push(text[start..i].to_owned());
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < text.len() {
        lines.push(text[start..].to_owned());
    }
    lines
}

/// Parses a single `key=value` property line, stripping `#` comments.
pub fn parse_line(line: &str) -> Option<(&str, &str)> {
    let without_comment = line.split_once('#').map_or(line, |(before, _)| before).trim();
    let (key, value) = without_comment.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || key.chars().any(char::is_whitespace) || value.is_empty() {
        return None;
    }
    Some((key, value))
}

pub struct PropertiesFile;

impl PropertiesFile {
    pub fn read_map(path: &Path) -> io::Result<BTreeMap<String, String>> {
        Ok(Self::map_of(&TextFile::read(path)?))
    }

    /// The properties of a text, as read_map reads them
    fn map_of(file: &TextFile) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for line in &file.lines {
            if let Some((key, value)) = parse_line(line) {
                map.insert(key.to_owned(), value.to_owned());
            }
        }
        map
    }

    pub fn store_map(path: &Path, map: &BTreeMap<String, String>) -> io::Result<()> {
        TextFile::write(path, &Self::text_of(map))
    }

    /// A new properties file with all of `map` (UTF-8, CRLF, sorted)
    fn text_of(map: &BTreeMap<String, String>) -> TextFile {
        let lines = map.iter().map(|(k, v)| format!("{k}={v}")).collect();
        TextFile {
            lines,
            newline: "\r\n".to_owned(),
            encoding: TextEncoding::Utf8 { bom: false },
        }
    }

    /// Updates only `keys` in `path`, preserving existing comments, formatting, line endings,
    /// encoding, unknown keys, and line order.
    pub fn update_keys(
        path: &Path,
        values: &BTreeMap<String, String>,
        keys: &[String],
    ) -> io::Result<()> {
        let updated = Self::updated(TextFile::read(path)?, values, keys);
        TextFile::write(path, &updated)
    }

    /// `file` with `keys` set to their `values` (see update_keys)
    fn updated(file: TextFile, values: &BTreeMap<String, String>, keys: &[String]) -> TextFile {
        let to_write: HashSet<&str> = keys.iter().map(String::as_str).collect();
        let mut written: HashSet<String> = HashSet::new();

        let mut new_lines: Vec<String> = file
            .lines
            .into_iter()
            .map(|line| match parse_line(&line) {
                Some((k, _)) if to_write.contains(k) => {
                    if let Some(val) = values.get(k) {
                        written.insert(k.to_owned());
                        let comment = match line.find('#') {
                            Some(idx) => format!(" {}", &line[idx..]),
                            None => String::new(),
                        };
                        format!("{k}={val}{comment}")
                    } else {
                        line
                    }
                }
                _ => line,
            })
            .collect();

        for key in keys {
            if written.insert(key.clone())
                && let Some(val) = values.get(key)
            {
                new_lines.push(format!("{key}={val}"));
            }
        }

        TextFile {
            lines: new_lines,
            newline: file.newline,
            encoding: file.encoding,
        }
    }
}

pub fn user_profile_dir() -> io::Result<PathBuf> {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "USERPROFILE"))
}

pub fn profile_path(dir: &Path, name: &str) -> PathBuf {
    if name == DEFAULT_PROFILE {
        dir.join(format!("{PROP_PREFIX}.{PROP_EXT}"))
    } else {
        dir.join(format!("{PROP_PREFIX}.{name}.{PROP_EXT}"))
    }
}

pub fn exclusions_path(dir: &Path, name: &str) -> PathBuf {
    if name == DEFAULT_PROFILE {
        dir.join(format!("{PROP_PREFIX}.{EXCLUSIONS_EXT}"))
    } else {
        dir.join(format!("{PROP_PREFIX}.{name}.{EXCLUSIONS_EXT}"))
    }
}

fn extract_profile_name(file_name: &str) -> Option<&str> {
    let stem = file_name
        .strip_prefix(".W10Wheel.")?
        .strip_suffix(".properties")?;
    if stem.is_empty()
        || stem.starts_with("--")
        || stem.eq_ignore_ascii_case(DEFAULT_PROFILE)
    {
        return None;
    }
    Some(stem)
}

pub fn list_profiles(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or_else(|_| entry.path().is_file()))
        .filter_map(|entry| {
            let name = entry.file_name();
            let s = name.to_str()?;
            extract_profile_name(s).map(str::to_owned)
        })
        .collect();
    names.sort();
    names
}

pub fn profile_exists(dir: &Path, name: &str) -> bool {
    profile_path(dir, name).is_file()
}

pub fn find_profile_name(dir: &Path, name: &str) -> Option<String> {
    if name == DEFAULT_PROFILE {
        return Some(DEFAULT_PROFILE.to_owned());
    }
    if !profile_exists(dir, name) {
        return None;
    }
    let names = list_profiles(dir);
    names
        .iter()
        .find(|n| n.as_str() == name)
        .or_else(|| names.iter().find(|n| n.eq_ignore_ascii_case(name)))
        .cloned()
        .or_else(|| Some(name.to_owned()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileNameError {
    LeadingOrTrailingSpaces,
    StartsWithDoubleDash,
    ReservedDefault,
    InvalidChars,
    AlreadyExists,
}

pub fn validate_profile_name(name: &str) -> Result<(), ProfileNameError> {
    if name.trim().is_empty() || name.trim() != name {
        return Err(ProfileNameError::LeadingOrTrailingSpaces);
    }
    if name.starts_with("--") {
        return Err(ProfileNameError::StartsWithDoubleDash);
    }
    if name.eq_ignore_ascii_case(DEFAULT_PROFILE) {
        return Err(ProfileNameError::ReservedDefault);
    }
    if name
        .chars()
        .any(|c| (c as u32) < 32 || INVALID_FILE_CHARS.contains(c))
    {
        return Err(ProfileNameError::InvalidChars);
    }
    Ok(())
}

pub fn validate_new_profile_name(dir: &Path, name: &str) -> Result<(), ProfileNameError> {
    validate_profile_name(name)?;
    if profile_exists(dir, name) {
        return Err(ProfileNameError::AlreadyExists);
    }
    Ok(())
}

/// The properties file of new properties (ConfigStore::add_profile), held open from its creation
/// until the Add is done. Created only if there is none (create_new): a file made there by another
/// program since the name was checked is not replaced (an AlreadyExists error), as .NET File.Copy
/// fails. Written whole at once with its final contents, through this handle, which shares nothing:
/// while it is held, no other program can open the file at all, so none can write, replace, rename
/// or delete it, and the file written, and the one deleted if the Add fails (discard), is always
/// this one. Not even reading is shared (nothing needs it: the Add loads what it wrote from
/// memory): a program reading it could map it into memory, and Windows does not delete a file
/// mapped so, which would leave the file of a failed Add behind, its name taken.
struct NewProfileFile {
    file: fs::File,
}

impl NewProfileFile {
    fn create(path: &Path, contents: &[u8]) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;

        let file = OpenOptions::new()
            // write: needed by OpenOptions for create_new (the access is access_mode)
            .write(true)
            .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE_ACCESS)
            .share_mode(0)
            .create_new(true)
            .open(path)?;
        let mut new = Self { file };
        let written = new.file.write_all(contents).and_then(|()| new.file.sync_all());
        if let Err(err) = written {
            let _ = new.discard();
            return Err(err);
        }
        Ok(new)
    }

    /// The Add failed: this file (not whatever is at its path) is deleted when the handle closes
    fn discard(self) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;

        // FILE_DISPOSITION_INFO { DeleteFile: TRUE }
        let delete: u8 = 1;
        let ok = unsafe {
            SetFileInformationByHandle(
                self.file.as_raw_handle(),
                FILE_DISPOSITION_INFO_CLASS,
                (&raw const delete).cast(),
                size_of::<u8>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetFileInformationByHandle(
        file: std::os::windows::io::RawHandle,
        class: u32,
        info: *const std::ffi::c_void,
        size: u32,
    ) -> i32;
}

const DELETE_ACCESS: u32 = 0x0001_0000;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_DISPOSITION_INFO_CLASS: u32 = 4;

pub fn delete_profile(dir: &Path, name: &str) -> io::Result<()> {
    if name == DEFAULT_PROFILE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Default profile cannot be deleted.",
        ));
    }
    fs::remove_file(profile_path(dir, name))?;
    // The properties are deleted: their excluded programs that cannot be deleted (e.g. the file
    // locked) are not an error, as in .NET (Ctx.deleteProperties). Else the deletion would stop
    // halfway, with the properties gone and still selected. The file left is a leftover that is
    // never loaded (no properties of its name), and replaced if properties of this name are added
    // again (ConfigStore::add_profile).
    let _ = fs::remove_file(exclusions_path(dir, name));
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreStatus {
    PropertiesUnreadableNotSaved,
    PropertiesSaveFailed,
    ExclusionsUnreadableNotSaved,
    ExclusionsSaveFailed,
    PropertiesReadFailed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartupLoadReport {
    pub missing_profile: Option<String>,
}

/// Manages active settings, profile switching, partial persistence, and read/write error states.
pub struct ConfigStore {
    dir: Option<PathBuf>,
    selected: String,
    config: Config,
    custom_table: CustomTableState,
    exclusions: ExclusionList,
    stored_exclusions: ExclusionList,
    system_lang: UiLanguage,
    loaded: bool,
    exclusions_loaded: bool,
    exclusions_rewrite: bool,
    stored_map: BTreeMap<String, String>,
    load_error: Option<String>,
    store_error: Option<String>,
    exclusions_load_error: Option<String>,
    exclusions_store_error: Option<String>,
}

impl ConfigStore {
    pub fn open_user(system_lang: UiLanguage) -> io::Result<Self> {
        let dir = user_profile_dir()?;
        Ok(Self::open_in(dir, system_lang))
    }

    pub fn open_in(dir: PathBuf, system_lang: UiLanguage) -> Self {
        let config = Config::default_for_language(system_lang);
        let stored_map = config.to_property_map();
        Self {
            dir: Some(dir),
            selected: DEFAULT_PROFILE.to_owned(),
            config,
            custom_table: CustomTableState::NotWritten,
            exclusions: ExclusionList::new(),
            stored_exclusions: ExclusionList::new(),
            system_lang,
            loaded: false,
            exclusions_loaded: false,
            exclusions_rewrite: false,
            stored_map,
            load_error: None,
            store_error: None,
            exclusions_load_error: None,
            exclusions_store_error: None,
        }
    }

    pub fn session_only(system_lang: UiLanguage, error: impl Into<String>) -> Self {
        let config = Config::default_for_language(system_lang);
        let stored_map = config.to_property_map();
        Self {
            dir: None,
            selected: DEFAULT_PROFILE.to_owned(),
            config,
            custom_table: CustomTableState::NotWritten,
            exclusions: ExclusionList::new(),
            stored_exclusions: ExclusionList::new(),
            system_lang,
            loaded: false,
            exclusions_loaded: false,
            exclusions_rewrite: false,
            stored_map,
            load_error: Some(error.into()),
            store_error: None,
            exclusions_load_error: None,
            exclusions_store_error: None,
        }
    }

    /// The language of the UI: the messages of the errors are in it
    fn lang(&self) -> UiLanguage {
        self.config.ui_language
    }

    fn no_dir_error(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            crate::locale::tr(self.lang(), "The settings folder is not available"),
        )
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn selected_profile(&self) -> &str {
        &self.selected
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn custom_table(&self) -> &CustomTableState {
        &self.custom_table
    }

    pub fn exclusions(&self) -> &ExclusionList {
        &self.exclusions
    }

    pub fn exclusions_mut(&mut self) -> &mut ExclusionList {
        &mut self.exclusions
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn exclusions_loaded(&self) -> bool {
        self.exclusions_loaded
    }

    pub fn load_error(&self) -> Option<&str> {
        self.load_error.as_deref()
    }

    pub fn store_error(&self) -> Option<&str> {
        self.store_error.as_deref()
    }

    pub fn exclusions_load_error(&self) -> Option<&str> {
        self.exclusions_load_error.as_deref()
    }

    pub fn exclusions_store_error(&self) -> Option<&str> {
        self.exclusions_store_error.as_deref()
    }

    /// Why the changes left unsaved could not be saved (has_unsaved_changes), as .NET tells it when
    /// asking before they are lost (Ctx.confirmUnstoredLoss): the error of the settings first if
    /// settings are left (that of the store, else of the load), else of the excluded programs
    pub fn unsaved_reason(&self) -> Option<&str> {
        let settings_unsaved =
            !changed_keys(&self.config.to_property_map(), &self.stored_map).is_empty();
        let of_settings = self.store_error.as_deref().or(self.load_error.as_deref());
        let of_exclusions = self
            .exclusions_store_error
            .as_deref()
            .or(self.exclusions_load_error.as_deref());
        if settings_unsaved {
            of_settings.or(of_exclusions)
        } else {
            of_exclusions.or(of_settings)
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        !changed_keys(&self.config.to_property_map(), &self.stored_map).is_empty()
            || self.exclusions != self.stored_exclusions
    }

    pub fn store_status(&self) -> Option<StoreStatus> {
        if !self.loaded {
            Some(StoreStatus::PropertiesUnreadableNotSaved)
        } else if self.store_error.is_some() {
            Some(StoreStatus::PropertiesSaveFailed)
        } else if !self.exclusions_loaded {
            Some(StoreStatus::ExclusionsUnreadableNotSaved)
        } else if self.exclusions_store_error.is_some() {
            Some(StoreStatus::ExclusionsSaveFailed)
        } else if self.load_error.is_some() {
            Some(StoreStatus::PropertiesReadFailed)
        } else {
            None
        }
    }

    pub fn status_message(&self) -> Option<&'static str> {
        self.store_status().map(|status| {
            crate::locale::tr(UiLanguage::Ja, crate::locale::store_status_key(status))
        })
    }

    pub fn load_startup(&mut self, requested_profile: Option<&str>) -> StartupLoadReport {
        let mut report = StartupLoadReport::default();
        let Some(dir) = self.dir.clone() else {
            return report;
        };

        let target = match requested_profile {
            Some(req) => match find_profile_name(&dir, req) {
                Some(resolved) => resolved,
                None => {
                    report.missing_profile = Some(req.to_owned());
                    DEFAULT_PROFILE.to_owned()
                }
            },
            None => DEFAULT_PROFILE.to_owned(),
        };

        self.selected = target.clone();
        match read_profile_map(&dir, &target, self.lang()) {
            Ok(map) => {
                let (config, custom_table) = Config::from_property_map(&map, self.system_lang);
                self.stored_map = config.to_property_map();
                self.config = config;
                self.custom_table = custom_table;
                self.loaded = true;
                self.load_error = None;
                self.store_error = None;
            }
            Err(err) => {
                let config = Config::default_for_language(self.system_lang);
                self.stored_map = config.to_property_map();
                self.config = config;
                self.custom_table = CustomTableState::NotWritten;
                self.loaded = false;
                self.load_error = Some(err.to_string());
                self.store_error = None;
            }
        }

        match ExclusionsFile::read(&exclusions_path(&dir, &target)) {
            Ok(list) => {
                self.exclusions = list.clone();
                self.stored_exclusions = list;
                self.exclusions_loaded = true;
                self.exclusions_rewrite = false;
                self.exclusions_load_error = None;
                self.exclusions_store_error = None;
            }
            Err(err) => {
                self.exclusions = ExclusionList::new();
                self.stored_exclusions = ExclusionList::new();
                self.exclusions_loaded = false;
                self.exclusions_rewrite = false;
                self.exclusions_load_error = Some(err.to_string());
                self.exclusions_store_error = None;
            }
        }
        report
    }

    pub fn store(&mut self) -> io::Result<()> {
        let Some(dir) = self.dir.clone() else {
            return Ok(());
        };

        let prop_result = if self.loaded {
            self.store_properties(&dir)
        } else {
            Ok(())
        };
        let excl_result = if self.exclusions_loaded {
            self.store_exclusions(&dir)
        } else {
            Ok(())
        };
        prop_result.and(excl_result)
    }

    fn store_properties(&mut self, dir: &Path) -> io::Result<()> {
        let path = profile_path(dir, &self.selected);
        let current = self.config.to_property_map();
        let changed = changed_keys(&current, &self.stored_map);
        let exists = path.exists();

        let result = if !exists && self.selected != DEFAULT_PROFILE {
            if !changed.is_empty() {
                Err(not_found_error(self.lang(), &self.selected))
            } else {
                Ok(())
            }
        } else if !exists {
            PropertiesFile::store_map(&path, &current)
        } else if changed.is_empty() {
            Ok(())
        } else {
            PropertiesFile::update_keys(&path, &current, &changed)
        };

        match result {
            Ok(()) => {
                self.stored_map = current;
                self.store_error = None;
                Ok(())
            }
            Err(err) => {
                self.store_error = Some(err.to_string());
                Err(err)
            }
        }
    }

    fn store_exclusions(&mut self, dir: &Path) -> io::Result<()> {
        if !self.exclusions_rewrite && self.exclusions == self.stored_exclusions {
            self.exclusions_store_error = None;
            return Ok(());
        }
        if !profile_path(dir, &self.selected).exists() && self.selected != DEFAULT_PROFILE {
            let err = not_found_error(self.lang(), &self.selected);
            self.exclusions_store_error = Some(err.to_string());
            return Err(err);
        }

        let excl_path = exclusions_path(dir, &self.selected);
        match ExclusionsFile::write(&excl_path, &self.exclusions) {
            Ok(()) => {
                self.stored_exclusions = self.exclusions.clone();
                self.exclusions_rewrite = false;
                self.exclusions_store_error = None;
                Ok(())
            }
            Err(err) => {
                self.exclusions_store_error = Some(err.to_string());
                Err(err)
            }
        }
    }

    pub fn reload(&mut self) -> io::Result<()> {
        let Some(dir) = self.dir.clone() else {
            return Err(self.no_dir_error());
        };
        let name = self.selected.clone();
        let map = match read_profile_map(&dir, &name, self.lang()) {
            Ok(map) => map,
            Err(err) => {
                self.load_error = Some(err.to_string());
                return Err(err);
            }
        };
        // The excluded programs that cannot be read (e.g. the file locked) do not keep the
        // properties from being loaded: those running are kept (see apply_loaded)
        let exclusions = ExclusionsFile::read(&exclusions_path(&dir, &name));
        self.apply_loaded(name, &map, exclusions);
        Ok(())
    }

    pub fn switch_profile(&mut self, name: &str) -> io::Result<()> {
        let Some(dir) = self.dir.clone() else {
            return Err(self.no_dir_error());
        };
        let map = read_profile_map(&dir, name, self.lang())?;
        let exclusions = ExclusionsFile::read(&exclusions_path(&dir, name));
        self.apply_loaded(name.to_owned(), &map, exclusions);
        Ok(())
    }

    pub fn add_profile(&mut self, name: &str) -> io::Result<()> {
        let Some(dir) = self.dir.clone() else {
            return Err(self.no_dir_error());
        };
        validate_new_profile_name(&dir, name).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                crate::locale::tr(self.lang(), crate::locale::profile_name_error_key(err)),
            )
        })?;

        let _ = self.store();
        let src_path = profile_path(&dir, &self.selected);
        let dest_path = profile_path(&dir, name);
        let dest_excl = exclusions_path(&dir, name);
        let current = self.config.to_property_map();
        let all_keys: Vec<String> = current.keys().cloned().collect();

        // The new properties, complete before their file is made: a copy of the selected ones
        // (their comments, their other keys, e.g. the custom table) with the settings running,
        // or all the settings if they have no file
        let contents = match TextFile::read(&src_path) {
            Ok(src) => PropertiesFile::updated(src, &current, &all_keys),
            Err(err) if err.kind() == io::ErrorKind::NotFound => PropertiesFile::text_of(&current),
            Err(err) => return Err(err),
        };

        // Made with them, and held until the Add is done (see NewProfileFile). Made by another
        // program since the name was checked: not replaced, the Add fails as for a name that exists
        let lang = self.lang();
        let new_file = NewProfileFile::create(&dest_path, &contents.to_bytes()).map_err(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    crate::locale::tr(
                        lang,
                        crate::locale::profile_name_error_key(ProfileNameError::AlreadyExists),
                    ),
                )
            } else {
                err
            }
        })?;

        // The excluded programs running, always, as .NET (Ctx.addProperties): an exclusions file
        // of this name without its properties is a leftover of earlier properties (e.g. deleted
        // while the file was locked, see delete_profile, or outside W10Wheel). It is replaced by
        // the list, or deleted if there is none, so that it does not come back as the list of the
        // new properties. Written whole or not at all (the old one stays).
        if let Err(err) = ExclusionsFile::write(&dest_excl, &self.exclusions) {
            // Not left half made: the properties file made by this Add is deleted (that file, held
            // since: nothing else can be at its path), they can be added again
            let _ = new_file.discard();
            return Err(err);
        }
        drop(new_file);

        // Loaded from what was written, not read again from the files (released now)
        let map = PropertiesFile::map_of(&contents);
        let exclusions = self.exclusions.clone();
        self.apply_loaded(name.to_owned(), &map, Ok(exclusions));
        Ok(())
    }

    pub fn delete_profile(&mut self, name: &str) -> io::Result<()> {
        if name == DEFAULT_PROFILE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                crate::locale::tr(self.lang(), "Default properties cannot be deleted"),
            ));
        }
        let Some(dir) = self.dir.clone() else {
            return Err(self.no_dir_error());
        };

        // Ensure Default properties can still be read before deleting the profile (its excluded
        // programs that cannot be read are none until they can, as at startup)
        let default_map = read_profile_map(&dir, DEFAULT_PROFILE, self.lang())?;
        let default_exclusions = ExclusionsFile::read(&exclusions_path(&dir, DEFAULT_PROFILE));
        delete_profile(&dir, name)?;
        self.apply_loaded(DEFAULT_PROFILE.to_owned(), &default_map, default_exclusions);
        Ok(())
    }

    /// The keyboard hook could not be installed: the keyboard trigger is turned off, as the
    /// settings dialog then shows. `on_load`: it was installed for a load of the properties
    /// (startup, Reload, switching...): the file is not changed for it (the hook is tried again at
    /// the next load or startup), as .NET does. Otherwise the user turned it on and it failed:
    /// stored as off by the next store, as any change.
    pub fn turn_off_keyboard_hook(&mut self, on_load: bool) {
        self.config.keyboard_hook = false;
        if on_load {
            self.stored_map.insert(
                crate::config::keys::KEYBOARD_HOOK.to_owned(),
                false.to_string(),
            );
        }
    }

    pub fn reset_to_defaults(&mut self) -> io::Result<()> {
        let lang = self.config.ui_language;
        self.config = Config::default_for_language(lang);
        self.stored_map.clear();
        self.exclusions.clear();
        self.exclusions_rewrite = true;
        self.store()
    }

    /// The properties of `name` applied as read (`map`), with their excluded programs. If those
    /// could not be read: at a Reload (the same properties) the list running is kept, with its
    /// changes not stored yet, as .NET does (Ctx.loadExclusions); else (switching) there are none.
    /// Their changes are not stored until the file can be read again (exclusions_loaded).
    fn apply_loaded(
        &mut self,
        name: String,
        map: &BTreeMap<String, String>,
        exclusions: io::Result<ExclusionList>,
    ) {
        let (config, custom_table) = Config::from_property_map(map, self.system_lang);
        let same_profile = self.selected == name;
        self.selected = name;
        self.stored_map = config.to_property_map();
        self.config = config;
        self.custom_table = custom_table;
        self.loaded = true;
        self.load_error = None;
        self.store_error = None;
        match exclusions {
            Ok(list) => {
                self.exclusions = list.clone();
                self.stored_exclusions = list;
                self.exclusions_loaded = true;
                self.exclusions_rewrite = false;
                self.exclusions_load_error = None;
                self.exclusions_store_error = None;
            }
            Err(err) => {
                self.exclusions_loaded = false;
                self.exclusions_load_error = Some(err.to_string());
                if !same_profile {
                    self.exclusions = ExclusionList::new();
                    self.stored_exclusions = ExclusionList::new();
                    self.exclusions_rewrite = false;
                    self.exclusions_store_error = None;
                }
            }
        }
    }
}

/// Other properties than Default that do not exist (e.g. renamed or deleted outside W10Wheel), in
/// the language of the UI, as .NET says it ("Properties does not exist: name")
fn not_found_error(lang: UiLanguage, name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{}: {name}", crate::locale::tr(lang, "Properties does not exist")),
    )
}

fn read_profile_map(
    dir: &Path,
    name: &str,
    lang: UiLanguage,
) -> io::Result<BTreeMap<String, String>> {
    let path = profile_path(dir, name);
    match PropertiesFile::read_map(&path) {
        Ok(map) => Ok(map),
        Err(err) if err.kind() == io::ErrorKind::NotFound && name == DEFAULT_PROFILE => {
            Ok(BTreeMap::new())
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(not_found_error(lang, name)),
        Err(err) => Err(err),
    }
}

fn changed_keys(
    current: &BTreeMap<String, String>,
    stored: &BTreeMap<String, String>,
) -> Vec<String> {
    current
        .iter()
        .filter(|&(k, v)| stored.get(k) != Some(v))
        .map(|(k, _)| k.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::os::windows::fs::{OpenOptionsExt, symlink_file};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::config::{Trigger, keys};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> io::Result<Self> {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "w10wheel-prop-test-{}-{timestamp}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parse_line_handles_comments_equals_spaces_and_blanks() {
        assert_eq!(parse_line("# comment"), None);
        assert_eq!(parse_line("   "), None);
        assert_eq!(parse_line("=value"), None);
        assert_eq!(parse_line("bad key = value"), None);
        assert_eq!(parse_line("horizontalScroll = # empty"), None);
        assert_eq!(
            parse_line("  horizontalScroll = True # trailing comment"),
            Some(("horizontalScroll", "True"))
        );
        assert_eq!(
            parse_line("customAccelThreshold = 1, 2, 3"),
            Some(("customAccelThreshold", "1, 2, 3"))
        );
        assert_eq!(
            parse_line("extra = a=b=c # comment"),
            Some(("extra", "a=b=c"))
        );
    }

    #[test]
    fn text_file_round_trips_all_boms_and_line_endings() -> io::Result<()> {
        let dir = TestDir::new()?;
        let encodings = [
            TextEncoding::Utf8 { bom: false },
            TextEncoding::Utf8 { bom: true },
            TextEncoding::Utf16Le,
            TextEncoding::Utf16Be,
            TextEncoding::Utf32Le,
            TextEncoding::Utf32Be,
        ];

        for (idx, (encoding, newline)) in encodings
            .into_iter()
            .zip(["\r\n", "\n", "\r", "\r\n", "\n", "\r"])
            .enumerate()
        {
            let path = dir.0.join(format!("enc-{idx}.properties"));
            let file = TextFile {
                lines: vec![
                    "# 日本語コメント".into(),
                    "firstTrigger=RightDragTrigger".into(),
                    String::new(),
                    "horizontalScroll=false".into(),
                ],
                newline: newline.into(),
                encoding,
            };
            TextFile::write(&path, &file)?;

            let reread = TextFile::read(&path)?;
            assert_eq!(reread, file);

            let map = PropertiesFile::read_map(&path)?;
            assert_eq!(map.get("firstTrigger").map(String::as_str), Some("RightDragTrigger"));
            assert_eq!(map.get("horizontalScroll").map(String::as_str), Some("false"));
        }
        Ok(())
    }

    #[test]
    fn partial_update_preserves_comments_order_unknown_keys_and_encoding() -> io::Result<()> {
        let dir = TestDir::new()?;
        let path = profile_path(&dir.0, DEFAULT_PROFILE);

        let initial = TextFile {
            lines: vec![
                "# Header comment".into(),
                String::new(),
                "customKey = keep=me # keep unknown".into(),
                "horizontalScroll = true # keep inline".into(),
                "customAccelThreshold = 1, 5, 10".into(),
                "customAccelMultiplier = 1.0, 2.0, 3.0".into(),
                "horizontalScroll = false # duplicate also updated".into(),
            ],
            newline: "\n".into(),
            encoding: TextEncoding::Utf16Le,
        };
        TextFile::write(&path, &initial)?;

        let mut new_values = BTreeMap::new();
        new_values.insert(keys::HORIZONTAL_SCROLL.into(), "true".into());
        new_values.insert(keys::REVERSE_SCROLL.into(), "true".into());

        PropertiesFile::update_keys(
            &path,
            &new_values,
            &[keys::HORIZONTAL_SCROLL.into(), keys::REVERSE_SCROLL.into()],
        )?;

        let updated = TextFile::read(&path)?;
        assert_eq!(updated.encoding, TextEncoding::Utf16Le);
        assert_eq!(updated.newline, "\n");
        assert_eq!(
            updated.lines,
            vec![
                "# Header comment",
                "",
                "customKey = keep=me # keep unknown",
                "horizontalScroll=true # keep inline",
                "customAccelThreshold = 1, 5, 10",
                "customAccelMultiplier = 1.0, 2.0, 3.0",
                "horizontalScroll=true # duplicate also updated",
                "reverseScroll=true",
            ]
        );
        Ok(())
    }

    #[test]
    fn write_rejects_readonly_and_cleans_up_temp_when_locked() -> io::Result<()> {
        let dir = TestDir::new()?;
        let ro_path = dir.0.join("ro.properties");
        let mut map = BTreeMap::new();
        map.insert("a".into(), "1".into());
        PropertiesFile::store_map(&ro_path, &map)?;

        let mut perms = fs::metadata(&ro_path)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&ro_path, perms)?;

        map.insert("a".into(), "2".into());
        let ro_err = PropertiesFile::store_map(&ro_path, &map).unwrap_err();
        assert_eq!(ro_err.kind(), io::ErrorKind::PermissionDenied);

        #[allow(clippy::permissions_set_readonly_false)]
        {
            let mut perms = fs::metadata(&ro_path)?.permissions();
            perms.set_readonly(false);
            fs::set_permissions(&ro_path, perms)?;
        }

        let locked_path = dir.0.join("locked.properties");
        PropertiesFile::store_map(&locked_path, &map)?;
        let original = fs::read(&locked_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_path)?;
        map.insert("a".into(), "3".into());
        assert!(PropertiesFile::store_map(&locked_path, &map).is_err());
        drop(lock);

        assert_eq!(fs::read(&locked_path)?, original);
        assert_eq!(fs::read_dir(&dir.0)?.count(), 2);
        Ok(())
    }

    #[test]
    fn write_follows_dangling_symlink_when_permitted() -> io::Result<()> {
        let dir = TestDir::new()?;
        let target = dir.0.join("real.properties");
        let link = dir.0.join("link.properties");
        if symlink_file(&target, &link).is_err() {
            // Symlink creation can require Developer Mode or elevation on Windows.
            return Ok(());
        }
        let mut map = BTreeMap::new();
        map.insert("horizontalScroll".into(), "false".into());
        PropertiesFile::store_map(&link, &map)?;
        assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert!(target.is_file());
        Ok(())
    }

    #[test]
    fn profile_discovery_and_name_validation_match_spec() -> io::Result<()> {
        let dir = TestDir::new()?;
        fs::write(dir.0.join(".W10Wheel.properties"), "")?;
        fs::write(dir.0.join(".W10Wheel.Default.properties"), "")?;
        fs::write(dir.0.join(".W10Wheel.default.properties"), "")?;
        fs::write(dir.0.join(".W10Wheel.--cmd.properties"), "")?;
        fs::write(dir.0.join(".W10Wheel.Gaming.properties"), "")?;
        fs::write(dir.0.join(".W10Wheel.Work.properties"), "")?;
        fs::create_dir(dir.0.join(".W10Wheel.Folder.properties"))?;

        assert_eq!(list_profiles(&dir.0), vec!["Gaming", "Work"]);
        assert_eq!(
            find_profile_name(&dir.0, "gaming"),
            Some("Gaming".to_owned())
        );
        assert_eq!(
            find_profile_name(&dir.0, DEFAULT_PROFILE),
            Some(DEFAULT_PROFILE.to_owned())
        );
        assert_eq!(find_profile_name(&dir.0, "Missing"), None);

        assert_eq!(
            validate_profile_name(" bad"),
            Err(ProfileNameError::LeadingOrTrailingSpaces)
        );
        assert_eq!(
            validate_profile_name("--test"),
            Err(ProfileNameError::StartsWithDoubleDash)
        );
        assert_eq!(
            validate_profile_name("deFaUlt"),
            Err(ProfileNameError::ReservedDefault)
        );
        assert_eq!(
            validate_profile_name("a:b"),
            Err(ProfileNameError::InvalidChars)
        );
        assert_eq!(
            validate_new_profile_name(&dir.0, "Gaming"),
            Err(ProfileNameError::AlreadyExists)
        );
        assert_eq!(validate_new_profile_name(&dir.0, "Custom"), Ok(()));
        Ok(())
    }

    #[test]
    fn config_store_lifecycle_protects_unloaded_files_and_supports_profiles() -> io::Result<()> {
        let dir = TestDir::new()?;

        // 1. Default absent at startup -> writes Default on first store.
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        let report = store.load_startup(None);
        assert_eq!(report.missing_profile, None);
        assert!(store.is_loaded());
        store.store()?;
        let default_path = profile_path(&dir.0, DEFAULT_PROFILE);
        assert!(default_path.is_file());

        // 2. .NET "True"/"False" & comments -> loading and calling store without edits is a no-op.
        let dotnet_text = "# .NET comment\r\nhorizontalScroll=True\r\nfirstTrigger=RightDrag\r\n";
        fs::write(&default_path, dotnet_text)?;
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        store.load_startup(None);
        store.store()?;
        assert_eq!(fs::read_to_string(&default_path)?, dotnet_text);

        // 3. Unreadable file at startup -> loaded=false, store() does not overwrite.
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&default_path)?;
        let mut locked_store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        locked_store.load_startup(None);
        assert!(!locked_store.is_loaded());
        assert!(locked_store.load_error().is_some());
        locked_store.config_mut().reverse_scroll = true;
        locked_store.store()?;
        drop(lock);
        assert_eq!(fs::read_to_string(&default_path)?, dotnet_text);

        // 4. Add profile preserves comments, switch & delete work, reset keeps ui_language.
        store.config_mut().ui_language = UiLanguage::Ja;
        store.config_mut().first_trigger = Trigger::Lr;
        store.add_profile("Gaming")?;
        assert_eq!(store.selected_profile(), "Gaming");
        let gaming_path = profile_path(&dir.0, "Gaming");
        let gaming_raw = fs::read_to_string(&gaming_path)?;
        assert!(gaming_raw.contains("# .NET comment"));

        store.reset_to_defaults()?;
        assert_eq!(store.config().ui_language, UiLanguage::Ja);
        assert_eq!(store.config().first_trigger, Trigger::RightDrag);
        assert!(fs::read_to_string(&gaming_path)?.contains("# .NET comment"));

        // Externally deleted named profile reports error on dirty store, succeeds when clean.
        fs::remove_file(&gaming_path)?;
        store.config_mut().swap_scroll = true;
        assert!(store.store().is_err());
        assert!(store.store_error().is_some());

        store.switch_profile(DEFAULT_PROFILE)?;
        assert_eq!(store.selected_profile(), DEFAULT_PROFILE);

        // Re-add and test delete_profile.
        store.add_profile("Temp")?;
        assert!(store.delete_profile(DEFAULT_PROFILE).is_err());
        store.delete_profile("Temp")?;
        assert_eq!(store.selected_profile(), DEFAULT_PROFILE);
        assert!(!profile_path(&dir.0, "Temp").exists());
        Ok(())
    }

    #[test]
    fn config_store_syncs_exclusions_with_profile_lifecycle() -> io::Result<()> {
        let dir = TestDir::new()?;
        let default_excl = exclusions_path(&dir.0, DEFAULT_PROFILE);
        fs::write(&default_excl, "C:\\Apps\\game.exe\r\n")?;

        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        store.load_startup(None);
        assert!(store.exclusions_loaded());
        assert_eq!(store.exclusions().entries(), &["C:\\Apps\\game.exe"]);
        assert!(!store.has_unsaved_changes());

        // Modifying exclusions marks dirty and writes back on store().
        assert!(store.exclusions_mut().add("C:\\Apps\\editor.exe"));
        assert!(store.has_unsaved_changes());
        store.store()?;
        assert!(!store.has_unsaved_changes());
        assert!(fs::read_to_string(&default_excl)?.contains("C:\\Apps\\editor.exe"));

        // add_profile copies exclusions; delete_profile removes both .properties and .exclusions.
        store.add_profile("Gaming")?;
        let gaming_excl = exclusions_path(&dir.0, "Gaming");
        assert!(gaming_excl.is_file());
        assert_eq!(ExclusionsFile::read(&gaming_excl)?.len(), 2);

        // Unreadable .exclusions protects existing file from overwrite.
        let excl_lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&gaming_excl)?;
        let mut locked = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        locked.load_startup(Some("Gaming"));
        assert!(locked.is_loaded());
        assert!(!locked.exclusions_loaded());
        assert!(locked.exclusions_load_error().is_some());
        locked.exclusions_mut().add("C:\\Apps\\should_not_save.exe");
        locked.store()?;
        drop(excl_lock);
        assert!(!fs::read_to_string(&gaming_excl)?.contains("should_not_save.exe"));

        // reload and reset_to_defaults clear exclusions on store().
        store.switch_profile("Gaming")?;
        store.exclusions_mut().clear();
        store.reload()?;
        assert_eq!(store.exclusions().len(), 2);
        store.reset_to_defaults()?;
        assert!(!gaming_excl.exists());

        // Writing a new exclusion on Gaming, then deleting Gaming deletes its .exclusions file too.
        assert!(store.exclusions_mut().add("C:\\Apps\\temp.exe"));
        store.store()?;
        assert!(gaming_excl.is_file());
        store.delete_profile("Gaming")?;
        assert!(!gaming_excl.exists());
        assert_eq!(store.selected_profile(), DEFAULT_PROFILE);
        assert_eq!(store.exclusions().len(), 2);

        // Recreated .exclusions while in-memory list is already empty is still removed by reset_to_defaults.
        let mut clean = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        clean.load_startup(None);
        clean.exclusions_mut().clear();
        clean.store()?;
        let default_excl = exclusions_path(&dir.0, DEFAULT_PROFILE);
        assert!(!default_excl.exists());
        fs::write(&default_excl, "C:\\Apps\\external.exe\r\n")?;
        clean.reset_to_defaults()?;
        assert!(!default_excl.exists());
        assert_eq!(clean.store_status(), None);
        Ok(())
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileAttributesW(file_name: *const u16, attributes: u32) -> i32;
    }

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;

    fn set_attributes(path: &Path, attributes: u32) -> io::Result<()> {
        if unsafe { SetFileAttributesW(wide_path(path).as_ptr(), attributes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// long_path with `absolute` given (a current directory of the test's own): independent of
    /// the one the tests run in
    fn long(path: &str, absolute: Option<&str>) -> String {
        let out = long_path(path.encode_utf16().collect(), |_| {
            absolute.map(|a| a.encode_utf16().collect())
        });
        String::from_utf16(&out).unwrap()
    }

    fn not_called(path: &str) -> String {
        let out = long_path(path.encode_utf16().collect(), |_| {
            panic!("made absolute: {path}")
        });
        String::from_utf16(&out).unwrap()
    }

    #[test]
    fn long_path_measures_utf16_units_with_the_nul() {
        // An absolute path of 246 units (247 with its NUL) is short, of 247 long. In Japanese:
        // one UTF-16 unit, three UTF-8 bytes.
        let at = |units: usize| format!(r"C:\{}", "あ".repeat(units - 3));
        assert_eq!(long("r.properties", Some(&at(246))), at(246));
        assert_eq!(long("r.properties", Some(&at(247))), format!(r"\\?\{}", at(247)));
        // The same boundary for the path given, short and absolute: passed with no call
        assert_eq!(not_called(&at(246)), at(246));
        assert_eq!(not_called(r"C:\a\b.properties"), r"C:\a\b.properties");
        assert_eq!(not_called("C:"), "C:");
        assert_eq!(not_called(r"\\server\share\a"), r"\\server\share\a");
        assert_eq!(not_called(""), "");
        let verbatim = format!(r"\\?\C:\{}", "x".repeat(300));
        assert_eq!(not_called(&verbatim), verbatim);
        let nt = format!(r"\??\C:\{}", "x".repeat(300));
        assert_eq!(not_called(&nt), nt);
    }

    #[test]
    fn long_path_makes_relative_paths_absolute() {
        // Long with the current directory only (Japanese in it)
        let cwd = format!(r"C:\{}", "作業".repeat(150));
        let full = format!(r"{cwd}\r.properties");
        assert_eq!(long("r.properties", Some(&full)), format!(r"\\?\{full}"));
        // Short with it: absolute, no prefix (as std)
        assert_eq!(long("r.properties", Some(r"C:\w\r.properties")), r"C:\w\r.properties");
        // Rooted on the current drive, or relative to a drive's directory: not absolute
        assert_eq!(long(r"\a.properties", Some(r"C:\a.properties")), r"C:\a.properties");
        assert_eq!(long("C:a.properties", Some(r"C:\w\a.properties")), r"C:\w\a.properties");
        // Cannot be made absolute: as it is, not prefixed
        let unnormalized = format!(r"{}\..\a.properties", "x".repeat(300));
        assert_eq!(long(&unnormalized, None), unnormalized);
    }

    #[test]
    fn wide_path_prefixes_paths_long_once_normalized() {
        let wide = |p: &str| String::from_utf16(&wide_path(Path::new(p))).unwrap();
        let long = "x".repeat(300);
        assert_eq!(wide(r"C:\a\b.properties"), "C:\\a\\b.properties\0");
        assert_eq!(wide(&format!(r"C:\{long}")), format!("\\\\?\\C:\\{long}\0"));
        assert_eq!(wide(&format!(r"C:\a\..\{long}")), format!("\\\\?\\C:\\{long}\0"));
        assert_eq!(wide(&format!(r"C:/a/{long}")), format!("\\\\?\\C:\\a\\{long}\0"));
        // Long, short once normalized: passed so
        assert_eq!(
            wide(&format!(r"C:\{long}\..\b.properties")),
            "C:\\b.properties\0"
        );
        // A file path as a device path (`\\.\`): `\\?\` if long, as it is if short
        assert_eq!(wide(&format!(r"\\.\C:\{long}")), format!("\\\\?\\C:\\{long}\0"));
        assert_eq!(wide(r"\\.\C:\a.properties"), "\\\\.\\C:\\a.properties\0");
        assert_eq!(
            wide(&format!(r"\\server\share\{long}")),
            format!("\\\\?\\UNC\\server\\share\\{long}\0")
        );
        assert_eq!(wide(&format!(r"\\?\C:\{long}")), format!("\\\\?\\C:\\{long}\0"));
    }

    /// The path as usually written, if it is a drive or a UNC path (with whether it is a drive
    /// path): `\\?\C:\…` as `C:\…`, `\\?\UNC\server\…` as `\\server\…`. None for any other, e.g.
    /// a volume GUID path (`\\?\Volume{…}\…`): with its prefix removed, it would be relative.
    fn plain_path(path: &Path) -> Option<(PathBuf, bool)> {
        use std::path::{Component, Prefix};
        let s = path.to_str()?;
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return None;
        };
        match prefix.kind() {
            Prefix::Disk(_) => Some((path.to_path_buf(), true)),
            Prefix::UNC(..) => Some((path.to_path_buf(), false)),
            Prefix::VerbatimDisk(_) => Some((PathBuf::from(s.strip_prefix(r"\\?\")?), true)),
            Prefix::VerbatimUNC(..) => {
                let rest = s.strip_prefix(r"\\?\UNC\")?;
                Some((PathBuf::from(format!(r"\\{rest}")), false))
            }
            _ => None,
        }
    }

    #[test]
    fn plain_path_converts_drive_and_unc_paths_only() {
        let plain = |p: &str| plain_path(Path::new(p)).map(|(p, d)| (p.display().to_string(), d));
        assert_eq!(plain(r"C:\t\a"), Some((r"C:\t\a".into(), true)));
        assert_eq!(plain(r"\\?\C:\t\a"), Some((r"C:\t\a".into(), true)));
        assert_eq!(plain(r"\\s\sh\a"), Some((r"\\s\sh\a".into(), false)));
        assert_eq!(plain(r"\\?\UNC\s\sh\a"), Some((r"\\s\sh\a".into(), false)));
        let volume = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\t\a";
        assert_eq!(plain(volume), None);
        assert_eq!(plain(r"\\.\C:\t\a"), None);
    }

    #[test]
    fn write_replaces_a_file_under_a_long_path() -> io::Result<()> {
        // Deeper than MAX_PATH: std creates it, ReplaceFileW needs the `\\?\` prefix
        let dir = TestDir::new()?;
        let mut deep = dir.0.clone();
        for idx in 0..6 {
            deep.push(format!("{idx}-{}", "d".repeat(50)));
        }
        fs::create_dir_all(&deep)?;
        let path = deep.join("long.properties");
        assert!(path.as_os_str().len() > 300);
        fs::write(&path, "a=0\r\n")?;
        let temporary = deep.join("long.properties.tmp");
        fs::write(&temporary, "a=1\r\n")?;
        replace_file(&temporary, &path, true)?;
        assert_eq!(fs::read_to_string(&path)?, "a=1\r\n");
        assert!(!temporary.exists());

        // The same file as a device path (`\\.\`), and through `..` (long, short once
        // normalized): replaced, not left to the direct write. Built from the directory as
        // usually written: TMP may be verbatim (`\\?\`), before which `\\.\` cannot be put and
        // where `..` is not resolved. Only for a drive or a UNC path: another (e.g. a volume GUID
        // path) has no such form, and the `\\.\` one only for a drive path.
        let Some((plain, drive)) = plain_path(&dir.0) else {
            return Ok(());
        };
        if drive {
            // As a string: join would replace `\\.\` by the absolute path
            let device = PathBuf::from(format!(
                r"\\.\{}",
                plain.join(path.strip_prefix(&dir.0).unwrap()).display()
            ));
            fs::write(&temporary, "a=2\r\n")?;
            replace_file(&temporary, &device, true)?;
            assert_eq!(fs::read_to_string(&path)?, "a=2\r\n");
        }
        let short = dir.0.join("short.properties");
        fs::write(&short, "b=0\r\n")?;
        let mut detour = plain;
        for _ in 0..60 {
            detour.push("x");
            detour.push("..");
        }
        let detour = detour.join("short.properties");
        assert!(detour.as_os_str().len() > 300);
        let short_tmp = dir.0.join("short.properties.tmp");
        fs::write(&short_tmp, "b=1\r\n")?;
        replace_file(&short_tmp, &detour, true)?;
        assert_eq!(fs::read_to_string(&short)?, "b=1\r\n");
        assert!(!short_tmp.exists());
        Ok(())
    }

    #[test]
    fn write_keeps_hidden_attribute_and_falls_back_to_direct_write() -> io::Result<()> {
        use std::os::windows::fs::MetadataExt;
        let dir = TestDir::new()?;
        let path = dir.0.join("hidden.properties");
        let mut map = BTreeMap::new();
        map.insert("a".into(), "1".into());
        PropertiesFile::store_map(&path, &map)?;

        // A hidden file stays hidden (ReplaceFile keeps the attributes)
        set_attributes(&path, FILE_ATTRIBUTE_HIDDEN)?;
        map.insert("a".into(), "2".into());
        PropertiesFile::store_map(&path, &map)?;
        assert_ne!(fs::metadata(&path)?.file_attributes() & FILE_ATTRIBUTE_HIDDEN, 0);
        assert_eq!(PropertiesFile::read_map(&path)?.get("a").map(String::as_str), Some("2"));
        set_attributes(&path, FILE_ATTRIBUTE_NORMAL)?;

        // Open in a program that shares reading and writing but not deleting: it cannot be
        // replaced, so it is written directly
        let open = OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2) // FILE_SHARE_READ | FILE_SHARE_WRITE
            .open(&path)?;
        map.insert("a".into(), "3".into());
        PropertiesFile::store_map(&path, &map)?;
        drop(open);
        assert_eq!(PropertiesFile::read_map(&path)?.get("a").map(String::as_str), Some("3"));
        // No temporary file left
        assert_eq!(fs::read_dir(&dir.0)?.count(), 1);
        Ok(())
    }

    #[test]
    fn unreadable_exclusions_do_not_keep_properties_from_loading() -> io::Result<()> {
        let dir = TestDir::new()?;
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        store.load_startup(None);
        assert!(store.exclusions_mut().add("C:\\Apps\\a.exe"));
        store.store()?;
        store.add_profile("Work")?;
        store.switch_profile(DEFAULT_PROFILE)?;

        let default_prop = profile_path(&dir.0, DEFAULT_PROFILE);
        fs::write(&default_prop, "firstTrigger=MiddleTrigger\r\n")?;
        let default_excl = exclusions_path(&dir.0, DEFAULT_PROFILE);
        let lock = OpenOptions::new().read(true).share_mode(0).open(&default_excl)?;

        // Reload: the properties are loaded, the excluded programs running are kept (not stored)
        store.reload()?;
        assert_eq!(store.config().first_trigger, Trigger::Middle);
        assert_eq!(store.exclusions().len(), 1);
        assert!(!store.exclusions_loaded());
        assert_eq!(store.store_status(), Some(StoreStatus::ExclusionsUnreadableNotSaved));

        // Switching to other properties and back: none until the file can be read
        store.switch_profile("Work")?;
        assert!(store.exclusions_loaded());
        store.switch_profile(DEFAULT_PROFILE)?;
        assert!(store.exclusions().is_empty());
        assert!(!store.exclusions_loaded());
        drop(lock);

        store.reload()?;
        assert!(store.exclusions_loaded());
        assert_eq!(store.exclusions().len(), 1);
        Ok(())
    }

    #[test]
    fn deleting_with_locked_exclusions_completes_and_the_leftover_never_comes_back() -> io::Result<()> {
        let dir = TestDir::new()?;
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        store.load_startup(None);
        store.add_profile("Work")?;
        assert!(store.exclusions_mut().add("C:\\Apps\\old.exe"));
        store.store()?;
        let work_prop = profile_path(&dir.0, "Work");
        let work_excl = exclusions_path(&dir.0, "Work");
        assert!(work_excl.is_file());

        // The exclusions locked: the deletion still completes, and Default is selected
        let lock = OpenOptions::new().read(true).share_mode(0).open(&work_excl)?;
        store.delete_profile("Work")?;
        drop(lock);
        assert!(!work_prop.exists());
        assert_eq!(store.selected_profile(), DEFAULT_PROFILE);
        // The list left behind is never loaded: no properties of its name
        assert!(work_excl.is_file());
        assert!(store.switch_profile("Work").is_err());

        // Added again with no excluded programs: the leftover is deleted, not taken back
        assert!(store.exclusions().is_empty());
        store.add_profile("Work")?;
        assert!(store.exclusions().is_empty());
        assert!(!work_excl.exists());
        Ok(())
    }

    #[test]
    fn adding_properties_replaces_a_leftover_exclusions_file_with_the_list_running() -> io::Result<()> {
        let dir = TestDir::new()?;
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        store.load_startup(None);
        assert!(store.exclusions_mut().add("C:\\Apps\\current.exe"));
        store.store()?;

        // A leftover of earlier properties named Work (their .properties gone)
        let work_excl = exclusions_path(&dir.0, "Work");
        fs::write(&work_excl, "C:\\Apps\\leftover.exe\r\n")?;

        store.add_profile("Work")?;
        assert_eq!(store.exclusions().entries(), ["C:\\Apps\\current.exe"]);
        assert_eq!(
            ExclusionsFile::read(&work_excl)?.entries(),
            ["C:\\Apps\\current.exe"]
        );

        // Cannot be replaced (locked): the Add fails as a whole, no properties left half made
        let dir2 = TestDir::new()?;
        let mut store2 = ConfigStore::open_in(dir2.0.clone(), UiLanguage::En);
        store2.load_startup(None);
        let locked_excl = exclusions_path(&dir2.0, "Home");
        fs::write(&locked_excl, "C:\\Apps\\leftover.exe\r\n")?;
        let lock = OpenOptions::new().read(true).share_mode(0).open(&locked_excl)?;
        assert!(store2.add_profile("Home").is_err());
        drop(lock);
        assert!(!profile_path(&dir2.0, "Home").exists());
        assert_eq!(store2.selected_profile(), DEFAULT_PROFILE);
        Ok(())
    }

    #[test]
    fn a_properties_file_made_by_another_program_meanwhile_is_not_replaced() -> io::Result<()> {
        let dir = TestDir::new()?;
        // Made by another program after the name was checked
        let dest = profile_path(&dir.0, "Work");
        fs::write(&dest, "# theirs\r\n")?;
        let err = NewProfileFile::create(&dest, b"# mine\r\n").err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&dest)?, "# theirs\r\n");
        Ok(())
    }

    #[test]
    fn the_new_properties_file_cannot_be_replaced_while_held_and_discard_deletes_only_it()
    -> io::Result<()> {
        let dir = TestDir::new()?;
        let path = profile_path(&dir.0, "Work");
        let mine = b"# mine\r\nreverseScroll=true\r\n";
        let new_file = NewProfileFile::create(&path, mine)?;

        // While held, no other program can open it: not even to read it (and map it into memory,
        // which would keep Windows from deleting it), nor replace it (also with the same contents:
        // another file), write into it, rename it or delete it
        assert!(fs::read(&path).is_err());
        let other = dir.0.join("other.properties");
        fs::write(&other, mine)?;
        assert!(fs::rename(&other, &path).is_err());
        assert!(PropertiesFile::store_map(&path, &BTreeMap::new()).is_err());
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::rename(&path, dir.0.join("moved.properties")).is_err());
        assert!(fs::remove_file(&path).is_err());
        assert!(path.is_file());

        // The Add failed: this file is deleted (nothing else can be at its path)
        new_file.discard()?;
        assert!(!path.exists());
        assert!(other.is_file());

        // The Add done: the file stays with its contents, free again
        let kept = NewProfileFile::create(&path, mine)?;
        drop(kept);
        assert_eq!(fs::read(&path)?, mine);
        fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn added_properties_are_a_copy_with_the_settings_running() -> io::Result<()> {
        let dir = TestDir::new()?;
        let default_prop = profile_path(&dir.0, DEFAULT_PROFILE);
        fs::write(
            &default_prop,
            "# my comment\r\ncustomAccelThreshold=1, 5\r\ncustomAccelMultiplier=1.0, 2.0\r\n",
        )?;
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        store.load_startup(None);
        store.config_mut().reverse_scroll = true;
        store.add_profile("Work")?;

        let text = fs::read_to_string(profile_path(&dir.0, "Work"))?;
        assert!(text.starts_with("# my comment\r\ncustomAccelThreshold=1, 5\r\n"));
        assert!(text.contains("reverseScroll=true\r\n"));
        assert_eq!(store.selected_profile(), "Work");
        assert!(store.config().reverse_scroll);
        assert!(matches!(store.custom_table(), CustomTableState::Loaded { .. }));
        assert!(!store.has_unsaved_changes());
        Ok(())
    }

    #[test]
    fn store_errors_are_in_the_ui_language() -> io::Result<()> {
        let dir = TestDir::new()?;
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::Ja);
        store.load_startup(None);
        assert_eq!(store.config().ui_language, UiLanguage::Ja);

        // Properties deleted outside W10Wheel, then switched to
        let err = store.switch_profile("Gone").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert_eq!(err.to_string(), "設定ファイルが存在しません: Gone");
        assert!(!err.to_string().contains("Properties does not exist"));

        // A read-only file: Windows' own error (access denied), not an English text of W10Wheel
        let path = profile_path(&dir.0, DEFAULT_PROFILE);
        store.config_mut().reverse_scroll = true;
        store.store()?;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms)?;
        store.config_mut().reverse_scroll = false;
        let err = store.store().unwrap_err();
        assert_eq!(err.raw_os_error(), Some(ERROR_ACCESS_DENIED));
        set_attributes(&path, FILE_ATTRIBUTE_NORMAL)?;
        Ok(())
    }

    #[test]
    fn keyboard_hook_failure_turns_the_setting_off_stored_or_not() -> io::Result<()> {
        let dir = TestDir::new()?;
        let path = profile_path(&dir.0, DEFAULT_PROFILE);
        fs::write(&path, "keyboardHook=true\r\n")?;
        let mut store = ConfigStore::open_in(dir.0.clone(), UiLanguage::En);
        store.load_startup(None);
        assert!(store.config().keyboard_hook);

        // At a load: off, the file not changed (tried again at the next startup)
        store.turn_off_keyboard_hook(true);
        assert!(!store.config().keyboard_hook);
        assert!(!store.has_unsaved_changes());
        store.store()?;
        assert_eq!(fs::read_to_string(&path)?, "keyboardHook=true\r\n");

        // Turned on in the dialog and failed: stored as off
        store.config_mut().keyboard_hook = true;
        store.store()?;
        store.turn_off_keyboard_hook(false);
        store.store()?;
        assert_eq!(
            PropertiesFile::read_map(&path)?.get(keys::KEYBOARD_HOOK).map(String::as_str),
            Some("false")
        );
        Ok(())
    }
}
