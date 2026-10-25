//! End-to-end encrypted sync: a [`Remote`] wrapper that encrypts every file,
//! and every file's name, before it leaves the device. The cloud only ever
//! holds scrambled data; only devices that know the sync passphrase can read it.
//!
//! * The key comes from the passphrase with Argon2id (like locked notes). Its
//!   salt and a check value live in `.marrow-encryption.json` in the cloud
//!   folder, so another device can tell whether its passphrase is right. The
//!   passphrase itself is never stored there.
//! * Contents are sealed with XChaCha20-Poly1305 and a fresh random nonce.
//!   The file's path is sealed inside it, so files can't be swapped around.
//! * Names: the path is sealed with a nonce derived from the path itself, so
//!   the same note gets the same scrambled name on every device. Names use
//!   lowercase base32, which is safe on case-insensitive drives. A path too
//!   long for that gets a short hashed name instead, and is read from inside
//!   the file.
//! * All files sit in one flat folder, so folder names and nesting don't show.

use std::collections::BTreeMap;
use std::io;
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::lock::{self, Cost};
use crate::mirror::{Remote, RemoteFile, SyncStop, Version};

/// The file in the cloud folder that says it's encrypted (hidden, so never synced).
pub const MARKER: &str = ".marrow-encryption.json";
const CHECK: &[u8] = b"marrow encrypted sync v1";
const AAD: &[u8] = b"marrow-sync-v1";
/// Longer scrambled names than this become hashed names (Windows paths max out at 260).
const MAX_NAME: usize = 140;

#[derive(Serialize, Deserialize)]
struct Marker {
    v: u32,
    salt: String,
    m: u32,
    t: u32,
    p: u32,
    /// CHECK sealed with the key: proves a passphrase is right.
    check: String,
}

struct Keys {
    names: XChaCha20Poly1305,
    /// Derives each name's nonce.
    name_nonce: Zeroizing<[u8; 32]>,
    data: XChaCha20Poly1305,
}

impl Keys {
    fn from(master: &lock::Key) -> Keys {
        let sub = |ctx: &str| Zeroizing::new(blake3::derive_key(ctx, master.bytes()));
        Keys {
            names: XChaCha20Poly1305::new(sub("marrow sync names v1").as_ref().into()),
            name_nonce: sub("marrow sync name nonces v1"),
            data: XChaCha20Poly1305::new(sub("marrow sync data v1").as_ref().into()),
        }
    }
}

/// Wraps another [`Remote`] so everything is encrypted on the way out.
pub struct Sealed {
    inner: Box<dyn Remote>,
    passphrase: Zeroizing<String>,
    keys: OnceLock<Keys>,
    /// Plain path → name in the cloud, from the last listing.
    names: Mutex<BTreeMap<String, String>>,
}

impl Sealed {
    pub fn new(inner: Box<dyn Remote>, passphrase: &str) -> Self {
        Sealed { inner, passphrase: Zeroizing::new(passphrase.to_string()), keys: OnceLock::new(), names: Mutex::new(BTreeMap::new()) }
    }

    /// Reads (or, in an empty folder, creates) the marker, and checks the passphrase.
    fn keys(&self) -> Result<&Keys, SyncStop> {
        if let Some(k) = self.keys.get() {
            return Ok(k);
        }
        let io = |e: io::Error| SyncStop::Io { message: e.to_string() };
        let keys = match self.inner.read(MARKER) {
            Ok(bytes) => {
                let m: Marker = serde_json::from_slice(&bytes).map_err(|_| SyncStop::Io { message: "the encryption settings in the cloud folder are damaged".into() })?;
                let salt: [u8; 16] = B64
                    .decode(&m.salt)
                    .ok()
                    .and_then(|s| s.try_into().ok())
                    .ok_or_else(|| SyncStop::Io { message: "the encryption settings in the cloud folder are damaged".into() })?;
                let master = lock::derive(&self.passphrase, salt, Cost { m: m.m, t: m.t, p: m.p }).map_err(|e| SyncStop::Io { message: e.to_string() })?;
                let keys = Keys::from(&master);
                let check = B64.decode(&m.check).unwrap_or_default();
                if open_bytes(&keys.data, &check).as_deref() != Some(CHECK) {
                    return Err(SyncStop::SignIn { message: "the sync passphrase isn't the one this cloud folder was encrypted with".into() });
                }
                keys
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound || e.to_string().contains("not found") => {
                // Never encrypt into a folder that already holds plain files.
                if !self.inner.list().map_err(io)?.is_empty() {
                    return Err(SyncStop::Io {
                        message: "that cloud folder already has files that aren't encrypted. Pick an empty folder for encrypted sync".into(),
                    });
                }
                let salt = lock::new_salt();
                let cost = Cost::default();
                let master = lock::derive(&self.passphrase, salt, cost).map_err(|e| SyncStop::Io { message: e.to_string() })?;
                let keys = Keys::from(&master);
                let marker = Marker { v: 1, salt: B64.encode(salt), m: cost.m, t: cost.t, p: cost.p, check: B64.encode(seal_bytes(&keys.data, CHECK)) };
                self.inner.write(MARKER, &serde_json::to_vec_pretty(&marker).map_err(io::Error::other).map_err(io)?).map_err(io)?;
                keys
            }
            Err(e) => return Err(io(e)),
        };
        Ok(self.keys.get_or_init(|| keys))
    }

    fn keys_io(&self) -> io::Result<&Keys> {
        self.keys().map_err(|e| io::Error::other(e.to_string()))
    }

    /// The scrambled name for a path (the same on every device).
    fn name_for(&self, keys: &Keys, path: &str) -> String {
        let iv = blake3::keyed_hash(&keys.name_nonce, path.as_bytes());
        let mut nonce = [0u8; 24];
        nonce[..16].copy_from_slice(&iv.as_bytes()[..16]);
        let ct = keys.names.encrypt(XNonce::from_slice(&nonce), Payload { msg: path.as_bytes(), aad: AAD }).expect("encrypt");
        let mut raw = nonce[..16].to_vec();
        raw.extend_from_slice(&ct);
        let name = format!("n{}", base32(&raw));
        if name.len() <= MAX_NAME {
            name
        } else {
            format!("h{}", base32(&iv.as_bytes()[16..32]))
        }
    }

    /// The path a scrambled name stands for (None for hashed names: read the file).
    fn path_of(&self, keys: &Keys, name: &str) -> Option<String> {
        let raw = unbase32(name.strip_prefix('n')?)?;
        if raw.len() < 16 + 16 {
            return None;
        }
        let mut nonce = [0u8; 24];
        nonce[..16].copy_from_slice(&raw[..16]);
        let plain = keys.names.decrypt(XNonce::from_slice(&nonce), Payload { msg: &raw[16..], aad: AAD }).ok()?;
        String::from_utf8(plain).ok()
    }

    fn cloud_name(&self, key: &str) -> io::Result<String> {
        if let Some(n) = self.names.lock().unwrap_or_else(|p| p.into_inner()).get(key) {
            return Ok(n.clone());
        }
        let keys = self.keys_io()?;
        Ok(self.name_for(keys, key))
    }
}

fn seal_bytes(cipher: &XChaCha20Poly1305, plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ct = cipher.encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad: AAD }).expect("encrypt");
    let mut out = Vec::with_capacity(1 + 24 + ct.len());
    out.push(1);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

fn open_bytes(cipher: &XChaCha20Poly1305, sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < 1 + 24 + 16 || sealed[0] != 1 {
        return None;
    }
    cipher.decrypt(XNonce::from_slice(&sealed[1..25]), Payload { msg: &sealed[25..], aad: AAD }).ok()
}

/// The file's path goes inside it: [path length, 2 bytes][path][data].
fn frame(path: &str, data: &[u8]) -> Vec<u8> {
    let p = path.as_bytes();
    let mut out = Vec::with_capacity(2 + p.len() + data.len());
    out.extend_from_slice(&(p.len() as u16).to_be_bytes());
    out.extend_from_slice(p);
    out.extend_from_slice(data);
    out
}

fn unframe(bytes: &[u8]) -> Option<(String, &[u8])> {
    let n = u16::from_be_bytes(bytes.get(..2)?.try_into().ok()?) as usize;
    let path = std::str::from_utf8(bytes.get(2..2 + n)?).ok()?.to_string();
    Some((path, &bytes[2 + n..]))
}

impl Remote for Sealed {
    fn id(&self) -> String {
        format!("{}#encrypted", self.inner.id())
    }

    fn check(&self) -> Result<(), SyncStop> {
        self.inner.check()?;
        self.keys().map(|_| ())
    }

    fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>> {
        let keys = self.keys_io()?;
        let mut out = BTreeMap::new();
        let mut names = BTreeMap::new();
        for (name, file) in self.inner.list()? {
            if name.contains('/') {
                continue; // everything Marrow writes is in one flat folder
            }
            let path = match self.path_of(keys, &name) {
                Some(p) => p,
                // A hashed (long) name: the path is inside the file.
                None if name.starts_with('h') => match self.inner.read(&name).ok().and_then(|b| open_bytes(&keys.data, &b)) {
                    Some(plain) => match unframe(&plain) {
                        Some((p, _)) => p,
                        None => continue,
                    },
                    None => continue,
                },
                // Not something Marrow sealed with this passphrase: leave it alone.
                None => continue,
            };
            names.insert(path.clone(), name);
            out.insert(path, file);
        }
        *self.names.lock().unwrap_or_else(|p| p.into_inner()) = names;
        Ok(out)
    }

    fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        let keys = self.keys_io()?;
        let sealed = self.inner.read(&self.cloud_name(key)?)?;
        let plain = open_bytes(&keys.data, &sealed).ok_or_else(|| io::Error::other(format!("{key}: couldn't decrypt it (damaged, or sealed with another passphrase)")))?;
        let (path, data) = unframe(&plain).ok_or_else(|| io::Error::other(format!("{key}: damaged")))?;
        if path != key {
            return Err(io::Error::other(format!("{key}: the file in the cloud belongs to another note")));
        }
        Ok(data.to_vec())
    }

    fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version> {
        let keys = self.keys_io()?;
        let name = self.cloud_name(key)?;
        let v = self.inner.write(&name, &seal_bytes(&keys.data, &frame(key, bytes)))?;
        self.names.lock().unwrap_or_else(|p| p.into_inner()).insert(key.to_string(), name);
        Ok(v)
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        let name = self.cloud_name(key)?;
        self.inner.delete(&name)?;
        self.names.lock().unwrap_or_else(|p| p.into_inner()).remove(key);
        Ok(())
    }
}

const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Lowercase base32 without padding: safe on case-insensitive drives.
fn base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in bytes {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn unbase32(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = ALPHABET.iter().position(|&a| a == c.to_ascii_lowercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Whether a cloud folder holds a Marrow-encrypted vault.
pub fn is_encrypted(remote: &dyn Remote) -> bool {
    remote.read(MARKER).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mirror::{FolderRemote, Mirror};
    use std::fs;
    use std::path::Path;

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    /// Every byte in the cloud folder, to check nothing readable is there.
    fn everything(dir: &Path) -> String {
        let mut all = String::new();
        for e in walkdir::WalkDir::new(dir).into_iter().flatten() {
            all.push_str(&e.path().to_string_lossy());
            if e.file_type().is_file() {
                all.push_str(&String::from_utf8_lossy(&fs::read(e.path()).unwrap()));
            }
        }
        all
    }

    #[test]
    fn base32_round_trips() {
        for n in 0..40 {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(unbase32(&base32(&bytes)).unwrap(), bytes);
        }
        assert!(base32(b"any bytes at all").chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn the_cloud_only_sees_scrambled_data() {
        let t = tempfile::tempdir().unwrap();
        let (pc, phone, cloud) = (t.path().join("pc"), t.path().join("phone"), t.path().join("cloud"));
        fs::create_dir_all(&phone).unwrap();
        fs::create_dir_all(&cloud).unwrap();
        write(&pc.join("Private/Medical history.md"), "Allergic to penicillin\n");
        write(&pc.join("Trip plan.md"), "Book the ferry to Halifax\n");
        let long = format!("Deep/{}/Long note.md", "x".repeat(120));
        write(&pc.join(&long), "long path\n");
        let sealed = |pass: &str| Box::new(Sealed::new(Box::new(FolderRemote::new(&cloud)), pass));
        let a = Mirror::with_remote(&pc, sealed("correct horse battery staple"));
        let rep = a.sync(false).unwrap();
        assert_eq!(rep.to_remote.len(), 3);

        // Nothing readable in the cloud: not the text, not the names, not the folders.
        let raw = everything(&cloud);
        for secret in ["penicillin", "Medical", "Private", "ferry", "Trip plan", "Deep", "Long note"] {
            assert!(!raw.contains(secret), "{secret} leaked");
        }
        assert!(cloud.join(MARKER).exists());

        // Another device with the passphrase gets everything back, folders and all.
        let b = Mirror::with_remote(&phone, sealed("correct horse battery staple"));
        b.sync(false).unwrap();
        assert_eq!(fs::read_to_string(phone.join("Private/Medical history.md")).unwrap(), "Allergic to penicillin\n");
        assert_eq!(fs::read_to_string(phone.join(&long)).unwrap(), "long path\n");

        // Edits on both devices still merge.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&pc.join("Trip plan.md"), "Book the ferry to Halifax\n\nPack warm clothes\n");
        write(&phone.join("Trip plan.md"), "Check the weather\n\nBook the ferry to Halifax\n");
        a.sync(false).unwrap();
        let rep = b.sync(false).unwrap();
        assert_eq!(rep.merged, vec!["Trip plan.md"]);
        a.sync(false).unwrap();
        let want = "Check the weather\n\nBook the ferry to Halifax\n\nPack warm clothes\n";
        assert_eq!(fs::read_to_string(pc.join("Trip plan.md")).unwrap(), want);

        // Deleted on the phone: gone from the cloud and from the computer.
        fs::remove_file(phone.join(&long)).unwrap();
        b.sync(false).unwrap();
        a.sync(false).unwrap();
        assert!(!pc.join(&long).exists());

        // The wrong passphrase is turned away before anything happens.
        let other = tempfile::tempdir().unwrap();
        let c = Mirror::with_remote(other.path(), sealed("wrong passphrase"));
        assert!(matches!(c.sync(false), Err(SyncStop::SignIn { .. })));
        assert_eq!(fs::read_dir(other.path()).unwrap().count(), 0);
    }

    #[test]
    fn never_mixes_with_plain_files() {
        let t = tempfile::tempdir().unwrap();
        let (vault, cloud) = (t.path().join("vault"), t.path().join("cloud"));
        write(&vault.join("A.md"), "a\n");
        write(&cloud.join("Plain note.md"), "already here, not encrypted\n");
        let m = Mirror::with_remote(&vault, Box::new(Sealed::new(Box::new(FolderRemote::new(&cloud)), "pass phrase")));
        let e = m.sync(false).unwrap_err();
        assert!(e.to_string().contains("aren't encrypted"), "{e}");
        assert_eq!(fs::read_to_string(cloud.join("Plain note.md")).unwrap(), "already here, not encrypted\n");
        assert!(!cloud.join(MARKER).exists());
    }

    #[test]
    fn a_file_moved_to_another_name_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let cloud = t.path().join("cloud");
        fs::create_dir_all(&cloud).unwrap();
        let s = Sealed::new(Box::new(FolderRemote::new(&cloud)), "pass phrase");
        s.check().unwrap();
        s.write("a.md", b"note a").unwrap();
        s.write("b.md", b"note b").unwrap();
        let keys = s.keys().unwrap();
        let (na, nb) = (s.name_for(keys, "a.md"), s.name_for(keys, "b.md"));
        // Someone with access to the cloud swaps the two files.
        let (ba, bb) = (fs::read(cloud.join(&na)).unwrap(), fs::read(cloud.join(&nb)).unwrap());
        fs::write(cloud.join(&na), bb).unwrap();
        fs::write(cloud.join(&nb), ba).unwrap();
        assert!(s.read("a.md").unwrap_err().to_string().contains("another note"));
    }
}
