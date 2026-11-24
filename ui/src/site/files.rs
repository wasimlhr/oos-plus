// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `GET /files` and `GET /files/<name>` (oos-plus): the files the scope saved (screenshots, waveforms, setups), read
//! only, from the save folder (`Ui::save_dir`) and nowhere else. A name is one path part: what could leave the folder
//! (a separator, `..`, a drive, a hidden file) is refused before the disk is asked, and only a regular file that is
//! not a link is listed or read.

use oos_remote::http::Body;
use std::path::Path;

/// The most files one listing names.
const LIST_MAX: usize = 1000;
/// The largest file one download takes: a record of the scope is far below this, a larger file is not a saved record.
pub const FILE_MAX: u64 = 128 << 20;

/// The content type of a file by its extension. Nothing a browser would run (no `text/html`, no script): an unknown
/// extension is bytes.
fn content_type(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "bmp" => "image/bmp",
        "jpg" | "jpeg" => "image/jpeg",
        "csv" => "text/csv; charset=utf-8",
        "txt" | "setup" | "oss" | "log" => "text/plain; charset=utf-8",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Whether `name` is one plain file name of the folder: no separator of either kind, no `..`, no leading dot (hidden
/// files, `.`), no `:` (a drive, a stream of a file on Windows), no control character, at most 255 bytes.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}

/// A path part as the client sent it: `%XX` taken (a name with a space), `+` left as it is. None for a `%` that is
/// not followed by two hex digits or for bytes that are not UTF-8.
fn decode(part: &str) -> Option<String> {
    let b = part.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The JSON of `GET /files`: `{"files":[{"name":..,"size":..,"modified":..}]}` by name, the regular files of `dir`
/// (not links, not hidden ones, not those whose name `GET /files/<name>` would refuse), `modified` in seconds since
/// 1970 (0 where the file system gives none). A folder that is not there lists none.
pub fn list(dir: &Path) -> String {
    let mut found: Vec<(String, u64, u64)> = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(name) = entry.file_name().into_string() else { continue };
        // symlink_metadata: a link to a file elsewhere is no file of the folder
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
        if !meta.is_file() || !plain_name(&name) {
            continue;
        }
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        found.push((name, meta.len(), modified.map_or(0, |d| d.as_secs())));
    }
    found.sort();
    found.truncate(LIST_MAX);
    let rows: Vec<String> = found
        .iter()
        .map(|(n, size, mtime)| format!("{{\"name\":{},\"size\":{size},\"modified\":{mtime}}}", json_string(n)))
        .collect();
    format!("{{\"files\":[{}]}}", rows.join(","))
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The answer of `GET /files/<name>` in the form of `Site::answer`: the file, read on a sender thread; or, as the
/// reason it failed (409), why not. `name` is the path after `/files/`.
pub fn download(dir: &Path, name: &str) -> (&'static str, Body, bool) {
    let fail = |why: &str| ("text/plain", Body::Now(why.as_bytes().to_vec()), true);
    let Some(name) = decode(name).filter(|n| plain_name(n)) else { return fail("Not a file name of the save folder") };
    let path = dir.join(&name);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() && m.len() <= FILE_MAX => {
            let read = move || std::fs::read(&path).unwrap_or_default();
            (content_type(&name), Body::Later(Box::new(read)), false)
        }
        Ok(m) if m.is_file() => fail("The file is too large to download"),
        _ => fail("No such file in the save folder"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(b: Body) -> Vec<u8> {
        match b {
            Body::Now(v) => v,
            Body::Later(make) => make(),
        }
    }

    fn folder(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("oos-files-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names_that_could_leave_the_folder_are_not_plain() {
        for bad in [
            "", ".", "..", "../x", "a/../b", "a/b", "a\\b", "..\\x", ".hidden", "C:x", "c:\\x", "x:stream", "a\0b",
            "a\nb",
        ] {
            assert!(!plain_name(bad), "{bad:?}");
        }
        assert!(!plain_name(&"a".repeat(256)));
        for good in ["scr-001.png", "setup1.txt", "my file.csv", "a.b.c"] {
            assert!(plain_name(good), "{good:?}");
        }
    }

    #[test]
    fn encoded_traversal_is_refused_after_the_decoding() {
        let d = folder("encoded");
        std::fs::write(d.join("a.csv"), "x").unwrap();
        for bad in
            ["%2e%2e%2fsecret", "..%2fsecret", "%2E%2E", "a%2fb", "a%5cb", "%00", "%zz", "%e0%80", "..", "", "x%3ay"]
        {
            let (_, b, failed) = download(&d, bad);
            assert!(failed, "{bad:?} must be refused");
            assert!(matches!(b, Body::Now(_)), "refused without reading");
        }
        let (ctype, b, failed) = download(&d, "a%2ecsv");
        assert!(!failed && ctype.starts_with("text/csv"));
        assert_eq!(body(b), b"x");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn lists_and_reads_the_regular_files_only() {
        let d = folder("list");
        std::fs::write(d.join("b.png"), [1u8, 2, 3]).unwrap();
        std::fs::write(d.join("a b.txt"), "hi").unwrap();
        std::fs::write(d.join(".hidden"), "no").unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::fs::write(d.join("sub").join("inner.txt"), "no").unwrap();
        let l = list(&d);
        assert!(l.starts_with("{\"files\":[{\"name\":\"a b.txt\",\"size\":2,\"modified\":"), "{l}");
        assert!(l.contains("\"name\":\"b.png\",\"size\":3"), "{l}");
        assert!(!l.contains("hidden") && !l.contains("sub") && !l.contains("inner"), "{l}");
        let (ctype, b, failed) = download(&d, "b.png");
        assert!(!failed && ctype == "image/png");
        assert_eq!(body(b), [1, 2, 3]);
        for missing in ["sub", "nothing.png", ".hidden", "sub%2finner.txt"] {
            assert!(download(&d, missing).2, "{missing}");
        }
        assert_eq!(list(&d.join("not-there")), "{\"files\":[]}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_neither_listed_nor_read() {
        let d = folder("link");
        let outside = folder("outside");
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), d.join("l.txt")).unwrap();
        assert_eq!(list(&d), "{\"files\":[]}");
        assert!(download(&d, "l.txt").2);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_string("a\"b\\c\u{1}"), "\"a\\\"b\\\\c\\u0001\"");
    }

    #[test]
    fn content_type_is_never_a_page() {
        assert_eq!(content_type("a.PNG"), "image/png");
        assert_eq!(content_type("a.html"), "application/octet-stream");
        assert_eq!(content_type("a.svg"), "application/octet-stream");
        assert_eq!(content_type("noext"), "application/octet-stream");
    }
}
