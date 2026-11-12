//! (NexiconDK, 12 November 2026) A company's own liveries shared through Bus Company Online's
//! server: a livery painted in the livery studio lives in the content folder of the computer it
//! was painted on (`omsihub_livery::export`: its textures and its `.cti`), and the other players
//! saw that company's buses in their white base ("min vens bemaling går ikke ind"). Now:
//!
//! - **Sent up** (the launcher's `bco_liveries`): when the owner or a co-owner saves a livery of
//!   the company, his launcher sends its files to the server, a part at a time (`PART`), only
//!   those the server lacks (`missing`: the files are kept by their SHA-256, so a livery saved
//!   again sends the textures that changed) - and a livery saved before this came is sent once.
//! - **Kept** on the server (`keep`, beside the companies in its data folder: `shared_liveries/`): per
//!   company its liveries - the name, the bus, the files (path, size, hash) -, the files once each
//!   (`blobs/<sha>`); what no livery names any more is swept away.
//! - **Fetched** (`bco_liveries::fetch_world`): before a game joins a map's world, its launcher
//!   fetches the liveries of every company driven there that it lacks or has an older one of,
//!   into openOMSI's content folder (never the OMSI 2 folder), the `.cti` last.
//!
//! What is taken (`check`): only a studio livery's files - `.dds` textures and `.cti` lists
//! under `Vehicles/`, in the studio's own names (`LiveryStudio`) -, at most `MAX_FILES` files,
//! `MAX_FILE` each and `MAX_TOTAL` together, `MAX_PER_COMPANY` liveries a company.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The livery studio's own names (`omsihub_livery::model::PREFIX`): its `.cti` begins
/// `~LiveryStudio_`, its textures lie in a folder `LiveryStudio/<nnnn>_<slug>`.
pub const PREFIX: &str = "LiveryStudio";
/// The bytes of a file sent or fetched at a time.
pub const PART: usize = 1 << 20;
pub const MAX_FILE: u64 = 64 << 20;
pub const MAX_TOTAL: u64 = 256 << 20;
pub const MAX_FILES: usize = 64;
pub const MAX_PER_COMPANY: usize = 40;

/// Why a livery or a part is turned away (English: keys of the translations).
pub const BAD_LIVERY: &str = "This is not a livery of the livery studio.";
pub const TOO_BIG: &str = "The livery is too big to share.";
pub const TOO_MANY: &str = "The company has too many liveries on the server.";
pub const NOT_ALL_THERE: &str = "Not all of the livery's files reached the server.";
pub const BAD_PART: &str = "A part of the livery's files did not arrive whole.";
pub const NO_FILE: &str = "The server has no such file.";
pub const NOT_OWNER: &str = "Only the company's owner and its co-owners share its liveries.";

/// A file of a livery: its path in the content folder (`Vehicles/…`, with `/`), its size and its
/// SHA-256 (hex).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct File {
    pub path: String,
    pub size: u64,
    pub sha: String,
}

/// A livery of a company kept on the server: its name in the game, the bus file it was painted
/// for, its files, who sent it and when (Unix seconds).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Shared {
    pub name: String,
    pub bus: String,
    pub files: Vec<File>,
    pub by: String,
    pub at: i64,
}

impl Shared {
    /// Whether it is the livery `name` (in any case).
    pub fn named(&self, name: &str) -> bool {
        self.name.trim().eq_ignore_ascii_case(name.trim())
    }

    /// Its files the same as `other`'s (paths in any case, hashes).
    pub fn same_files(&self, other: &[File]) -> bool {
        let key = |fs: &[File]| {
            let mut v: Vec<(String, String)> = fs.iter().map(|f| (f.path.to_ascii_lowercase(), f.sha.to_ascii_lowercase())).collect();
            v.sort();
            v
        };
        key(&self.files) == key(other)
    }
}

/// SHA-256 of `bytes` as hex.
pub fn sha_of(bytes: &[u8]) -> String {
    omsi_net::http::sha256(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `s` is a SHA-256 in hex (lower case).
pub fn sha_ok(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A livery's file's path as it is kept (`/`, no leading one) - None when it is no file of a
/// studio livery under `Vehicles/`: no `..`, no drive, a `.dds` or a `.cti` in the studio's names.
pub fn clean_path(p: &str) -> Option<String> {
    let p = p.trim().replace('\\', "/");
    if p.is_empty() || p.len() > 260 || p.starts_with('/') || p.contains(':') || p.chars().any(|c| c.is_control()) {
        return None;
    }
    let parts: Vec<&str> = p.split('/').collect();
    if parts.len() < 3 || parts.iter().any(|x| x.is_empty() || *x == "." || *x == "..") || !parts[0].eq_ignore_ascii_case("Vehicles") {
        return None;
    }
    let file = parts.last()?.to_ascii_lowercase();
    let studio = PREFIX.to_ascii_lowercase();
    let ok = if file.ends_with(".cti") {
        file.starts_with(&format!("~{studio}_"))
    } else if file.ends_with(".dds") {
        parts[..parts.len() - 1].iter().any(|x| x.eq_ignore_ascii_case(PREFIX))
    } else {
        false
    };
    ok.then_some(p)
}

/// Whether `s` may be kept: a name, a bus, the studio's files within the limits, a `.cti` among
/// them, each path once. The paths cleaned.
pub fn check(s: &mut Shared) -> Result<(), &'static str> {
    if s.name.trim().is_empty() || s.name.chars().count() > 64 || s.bus.trim().is_empty() || s.bus.len() > 260 {
        return Err(BAD_LIVERY);
    }
    if s.files.is_empty() || s.files.len() > MAX_FILES {
        return Err(if s.files.len() > MAX_FILES { TOO_BIG } else { BAD_LIVERY });
    }
    let mut seen = std::collections::HashSet::new();
    let mut total = 0u64;
    for f in s.files.iter_mut() {
        f.path = clean_path(&f.path).ok_or(BAD_LIVERY)?;
        f.sha = f.sha.trim().to_ascii_lowercase();
        if !sha_ok(&f.sha) || f.size == 0 || !seen.insert(f.path.to_ascii_lowercase()) {
            return Err(BAD_LIVERY);
        }
        if f.size > MAX_FILE {
            return Err(TOO_BIG);
        }
        total += f.size;
    }
    if total > MAX_TOTAL {
        return Err(TOO_BIG);
    }
    if !s.files.iter().any(|f| f.path.to_ascii_lowercase().ends_with(".cti")) {
        return Err(BAD_LIVERY);
    }
    Ok(())
}

// --- the server's store -----------------------------------------------------------------------

fn dir(data: &Path) -> PathBuf {
    data.join("shared_liveries")
}

fn blobs(data: &Path) -> PathBuf {
    dir(data).join("blobs")
}

/// The file of a hash (`sha` checked first).
fn blob(data: &Path, sha: &str) -> PathBuf {
    blobs(data).join(sha)
}

/// The list of company `company`'s liveries (its id: letters, digits, `-` and `_` only).
fn list_file(data: &Path, company: &str) -> Option<PathBuf> {
    let id = company.trim();
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')).then(|| dir(data).join(format!("{id}.json")))
}

/// Company `company`'s liveries on the server.
pub fn list(data: &Path, company: &str) -> Vec<Shared> {
    list_file(data, company).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The hashes of `s`'s files the server does not have yet (each once).
pub fn missing(data: &Path, s: &Shared) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in &s.files {
        if !blob(data, &f.sha).is_file() && !out.contains(&f.sha) {
            out.push(f.sha.clone());
        }
    }
    out
}

/// A part of the file of hash `sha` (`size` bytes in all): `bytes` at `offset` - the parts in
/// order, the first at 0. Whether the file is whole now (checked against its hash: a file that is
/// not is thrown away).
pub fn put_part(data: &Path, sha: &str, size: u64, offset: u64, bytes: &[u8]) -> Result<bool, &'static str> {
    use std::io::Write;
    let sha = sha.trim().to_ascii_lowercase();
    if !sha_ok(&sha) || size == 0 || bytes.is_empty() {
        return Err(BAD_PART);
    }
    if size > MAX_FILE {
        return Err(TOO_BIG);
    }
    if blob(data, &sha).is_file() {
        return Ok(true);
    }
    let _ = std::fs::create_dir_all(blobs(data));
    let part = blobs(data).join(format!("{sha}.part"));
    let have = if offset == 0 { 0 } else { std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) };
    if have != offset || offset + bytes.len() as u64 > size {
        let _ = std::fs::remove_file(&part);
        return Err(BAD_PART);
    }
    let mut f = std::fs::OpenOptions::new().create(true).write(true).append(offset != 0).truncate(offset == 0).open(&part).map_err(|_| BAD_PART)?;
    f.write_all(bytes).map_err(|_| BAD_PART)?;
    drop(f);
    if offset + (bytes.len() as u64) < size {
        return Ok(false);
    }
    let whole = std::fs::read(&part).map_err(|_| BAD_PART)?;
    if sha_of(&whole) != sha {
        let _ = std::fs::remove_file(&part);
        return Err(BAD_PART);
    }
    std::fs::rename(&part, blob(data, &sha)).map_err(|_| BAD_PART)?;
    Ok(true)
}

/// `len` bytes (at most `PART`) of the file of hash `sha` from `offset`, and its size.
pub fn read_part(data: &Path, sha: &str, offset: u64, len: usize) -> Result<(Vec<u8>, u64), &'static str> {
    use std::io::{Read, Seek, SeekFrom};
    let sha = sha.trim().to_ascii_lowercase();
    if !sha_ok(&sha) {
        return Err(NO_FILE);
    }
    let mut f = std::fs::File::open(blob(data, &sha)).map_err(|_| NO_FILE)?;
    let size = f.metadata().map_err(|_| NO_FILE)?.len();
    if offset > size {
        return Err(NO_FILE);
    }
    f.seek(SeekFrom::Start(offset)).map_err(|_| NO_FILE)?;
    let mut out = Vec::new();
    f.take(len.clamp(1, PART) as u64).read_to_end(&mut out).map_err(|_| NO_FILE)?;
    Ok((out, size))
}

/// Livery `s` kept as company `company`'s (sent by `by`, at `at`): in place of its livery of that
/// name - every file of it on the server first. What no livery names any more is swept away.
pub fn keep(data: &Path, company: &str, mut s: Shared, by: &str, at: i64) -> Result<(), &'static str> {
    check(&mut s)?;
    let path = list_file(data, company).ok_or(BAD_LIVERY)?;
    if !missing(data, &s).is_empty() {
        return Err(NOT_ALL_THERE);
    }
    s.name = s.name.trim().to_string();
    s.by = by.to_string();
    s.at = at;
    let mut all = list(data, company);
    match all.iter().position(|x| x.named(&s.name)) {
        Some(k) => all[k] = s,
        None if all.len() >= MAX_PER_COMPANY => return Err(TOO_MANY),
        None => all.push(s),
    }
    let _ = std::fs::create_dir_all(dir(data));
    std::fs::write(&path, serde_json::to_vec_pretty(&all).unwrap_or_default()).map_err(|_| NOT_ALL_THERE)?;
    sweep(data, at);
    Ok(())
}

/// The files no livery of any company names any more, and parts left a day.
pub fn sweep(data: &Path, now: i64) {
    let mut named = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(dir(data)) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                let all: Vec<Shared> = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
                named.extend(all.into_iter().flat_map(|s| s.files.into_iter().map(|f| f.sha)));
            }
        }
    }
    let Ok(rd) = std::fs::read_dir(blobs(data)) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let old = || {
            let modified = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            now - modified > 24 * 3600
        };
        let gone = match name.strip_suffix(".part") {
            Some(_) => old(),
            None => sha_ok(&name) && !named.contains(&name),
        };
        if gone {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

// --- base64, for the parts in the JSON of the server's door ---------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("shared-livery-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn livery(files: &[(&str, &[u8])]) -> Shared {
        Shared { name: "Umove Orange".into(), bus: "Vehicles/MAN_SD/MAN_SD.bus".into(), files: files.iter().map(|(p, b)| File { path: p.to_string(), size: b.len() as u64, sha: sha_of(b) }).collect(), ..Default::default() }
    }

    #[test]
    fn only_a_studio_livery_s_files_are_taken() {
        assert_eq!(clean_path("Vehicles\\MAN_SD\\Texture\\~LiveryStudio_0003_umove.cti").as_deref(), Some("Vehicles/MAN_SD/Texture/~LiveryStudio_0003_umove.cti"));
        assert!(clean_path("Vehicles/MAN_SD/Texture/LiveryStudio/0003_umove/body_1a2b3c4d.dds").is_some());
        for bad in [
            "Vehicles/MAN_SD/Texture/../../../Windows/x.dds",
            "C:/Vehicles/MAN_SD/LiveryStudio/x.dds",
            "/Vehicles/MAN_SD/LiveryStudio/x.dds",
            "Plugins/LiveryStudio/x.dds",
            "Vehicles/MAN_SD/Texture/body.dds",
            "Vehicles/MAN_SD/Texture/LiveryStudio/0003_umove/evil.exe",
            "Vehicles/MAN_SD/Texture/other.cti",
            "Vehicles//LiveryStudio/x.dds",
        ] {
            assert_eq!(clean_path(bad), None, "{bad}");
        }
        let mut s = livery(&[("Vehicles/A/Texture/LiveryStudio/0001_x/a.dds", b"aaaa")]);
        assert_eq!(check(&mut s), Err(BAD_LIVERY), "no .cti");
        let mut s = livery(&[("Vehicles/A/Texture/LiveryStudio/0001_x/a.dds", b"aaaa"), ("Vehicles/A/Texture/~LiveryStudio_0001_x.cti", b"[CTCTexture]")]);
        assert_eq!(check(&mut s), Ok(()));
        s.files[0].size = MAX_FILE + 1;
        assert_eq!(check(&mut s), Err(TOO_BIG));
    }

    #[test]
    fn a_livery_is_sent_in_parts_kept_and_fetched_again() {
        let data = tmp("keep");
        let big: Vec<u8> = (0..(PART * 2 + 17)).map(|i| (i * 7 % 251) as u8).collect();
        let cti = b"[CTCTexture]\nbody.dds\nLiveryStudio\\0001_x\\a.dds".to_vec();
        let s = livery(&[("Vehicles/A/Texture/LiveryStudio/0001_x/a.dds", &big), ("Vehicles/A/Texture/~LiveryStudio_0001_x.cti", &cti)]);
        assert_eq!(missing(&data, &s).len(), 2);
        assert_eq!(keep(&data, "c1", s.clone(), "Mathias", 100), Err(NOT_ALL_THERE));
        // the big one in three parts; a part out of order is turned away
        let sha = sha_of(&big);
        assert_eq!(put_part(&data, &sha, big.len() as u64, PART as u64, &big[PART..PART * 2]), Err(BAD_PART));
        let mut off = 0;
        for c in big.chunks(PART) {
            let done = put_part(&data, &sha, big.len() as u64, off as u64, c).unwrap();
            off += c.len();
            assert_eq!(done, off == big.len());
        }
        // a file that is not what its hash says is thrown away
        assert_eq!(put_part(&data, &sha_of(b"other"), cti.len() as u64, 0, &cti), Err(BAD_PART));
        assert!(put_part(&data, &sha_of(&cti), cti.len() as u64, 0, &cti).unwrap());
        assert!(missing(&data, &s).is_empty());
        keep(&data, "c1", s.clone(), "Mathias", 100).unwrap();
        let kept = list(&data, "c1");
        assert_eq!((kept.len(), kept[0].by.as_str(), kept[0].same_files(&s.files)), (1, "Mathias", true));
        // fetched in parts, the same bytes
        let mut back = Vec::new();
        loop {
            let (b, size) = read_part(&data, &sha, back.len() as u64, PART).unwrap();
            assert_eq!(size, big.len() as u64);
            back.extend(b);
            if back.len() as u64 >= size {
                break;
            }
        }
        assert_eq!(back, big);
        assert_eq!(read_part(&data, "../../etc", 0, 10), Err(NO_FILE));
        // saved again with another texture: in place of it, the old texture swept away
        let s2 = livery(&[("Vehicles/A/Texture/LiveryStudio/0001_x/b.dds", b"new texture"), ("Vehicles/A/Texture/~LiveryStudio_0001_x.cti", &cti)]);
        assert!(put_part(&data, &sha_of(b"new texture"), 11, 0, b"new texture").unwrap());
        keep(&data, "c1", Shared { name: "umove orange".into(), ..s2 }, "Mathias", 200).unwrap();
        assert_eq!(list(&data, "c1").len(), 1);
        assert!(read_part(&data, &sha, 0, 10).is_err(), "no livery names it any more");
        assert!(read_part(&data, &sha_of(&cti), 0, 10).is_ok());
        assert!(list_file(&data, "../x").is_none());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn base64_goes_there_and_back() {
        for n in 0..10 {
            let b: Vec<u8> = (0..n).map(|i| (i * 37) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&b)).unwrap(), b);
        }
        assert_eq!(b64_encode(b"Man"), "TWFu");
        assert_eq!(b64_encode(b"Ma"), "TWE=");
    }
}
