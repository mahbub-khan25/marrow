//! Password-locked notes.
//!
//! A locked note is still a `.md` file with the same name, so links to it,
//! renames and sync keep working, but its text is encrypted:
//!
//! ````text
//! %% Locked with Marrow. Open this note in Marrow and enter its password to read it. %%
//!
//! ```marrow-locked
//! v1 argon2id m=65536 t=3 p=1 salt=… nonce=…
//! hint …                      (optional, base64)
//! …ciphertext, base64, 64 characters a line…
//! ```
//! ````
//!
//! The key comes from the password with Argon2id (64 MiB, 3 passes) and the
//! text is sealed with XChaCha20-Poly1305. The header lines are authenticated
//! too, so a changed hint or parameter is caught. Every save gets a fresh
//! random nonce; notes locked together (a folder) share a salt, so one
//! password unlocks them with one key derivation.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

pub const FENCE: &str = "```marrow-locked";
const NOTICE: &str = "%% Locked with Marrow. Open this note in Marrow and enter its password to read it. %%";

/// What went wrong opening or reading a locked note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// The password (key) doesn't open this note.
    WrongPassword,
    /// The locked block is missing or has been edited by hand.
    Damaged(&'static str),
    /// The text isn't locked at all.
    NotLocked,
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::WrongPassword => f.write_str("That password doesn't open this note."),
            LockError::Damaged(why) => write!(f, "This locked note is damaged ({why}), so it can't be opened."),
            LockError::NotLocked => f.write_str("This note isn't locked."),
        }
    }
}

impl std::error::Error for LockError {}

/// Cost settings for turning a password into a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cost {
    /// Memory in KiB.
    pub m: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for Cost {
    fn default() -> Self {
        Cost { m: 64 * 1024, t: 3, p: 1 }
    }
}

/// A key made from a password, with the salt and cost it was made with.
#[derive(Clone)]
pub struct Key {
    bytes: Zeroizing<[u8; 32]>,
    pub salt: [u8; 16],
    pub cost: Cost,
}

impl Key {
    /// The raw key, for deriving the keys encrypted sync uses.
    pub(crate) fn bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// The salt as written in the file, used to find a cached key.
    pub fn salt_id(&self) -> String {
        B64.encode(self.salt)
    }
}

/// A fresh random salt.
pub fn new_salt() -> [u8; 16] {
    use chacha20poly1305::aead::rand_core::RngCore;
    let mut s = [0u8; 16];
    OsRng.fill_bytes(&mut s);
    s
}

/// Turns a password into a key (slow on purpose: about a quarter of a second).
pub fn derive(password: &str, salt: [u8; 16], cost: Cost) -> Result<Key, LockError> {
    let params = Params::new(cost.m, cost.t, cost.p, Some(32)).map_err(|_| LockError::Damaged("bad settings"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut bytes = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), &salt, bytes.as_mut())
        .map_err(|_| LockError::Damaged("bad settings"))?;
    Ok(Key { bytes, salt, cost })
}

/// The parts of a locked note's block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub salt: [u8; 16],
    pub nonce: [u8; 24],
    pub cost: Cost,
    pub hint: Option<String>,
    /// The header lines exactly as written; they're authenticated.
    aad: String,
    body: Vec<u8>,
}

impl Header {
    pub fn salt_id(&self) -> String {
        B64.encode(self.salt)
    }
}

/// Whether the text is a locked note.
pub fn is_locked(text: &str) -> bool {
    block(text).is_some()
}

/// The lines inside the ```marrow-locked fence.
fn block(text: &str) -> Option<Vec<&str>> {
    let mut lines = text.lines();
    // The fence must be the first thing after the notice (and blank lines), so
    // a note that merely shows the syntax in a code block isn't "locked".
    let first = loop {
        let l = lines.next()?.trim_end();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        break l;
    };
    if first != FENCE {
        return None;
    }
    let mut inside = Vec::new();
    for l in lines.by_ref() {
        let l = l.trim_end();
        if l == "```" {
            // Nothing but blank lines may follow.
            return lines.all(|r| r.trim().is_empty()).then_some(inside);
        }
        inside.push(l);
    }
    None
}

/// Reads the block's header (salt, cost, hint) without the password.
pub fn header(text: &str) -> Result<Header, LockError> {
    let lines = block(text).ok_or(LockError::NotLocked)?;
    let mut it = lines.into_iter();
    let first = it.next().ok_or(LockError::Damaged("no header"))?;
    let mut parts = first.split_whitespace();
    if parts.next() != Some("v1") || parts.next() != Some("argon2id") {
        return Err(LockError::Damaged("unknown format"));
    }
    let (mut m, mut t, mut p, mut salt, mut nonce) = (None, None, None, None, None);
    for kv in parts {
        let (k, v) = kv.split_once('=').ok_or(LockError::Damaged("bad header"))?;
        match k {
            "m" => m = v.parse::<u32>().ok(),
            "t" => t = v.parse::<u32>().ok(),
            "p" => p = v.parse::<u32>().ok(),
            "salt" => salt = B64.decode(v).ok().and_then(|b| <[u8; 16]>::try_from(b).ok()),
            "nonce" => nonce = B64.decode(v).ok().and_then(|b| <[u8; 24]>::try_from(b).ok()),
            _ => {}
        }
    }
    let cost = Cost {
        m: m.ok_or(LockError::Damaged("bad header"))?,
        t: t.ok_or(LockError::Damaged("bad header"))?,
        p: p.ok_or(LockError::Damaged("bad header"))?,
    };
    // Refuse absurd costs from a tampered file instead of hanging or running out of memory.
    if cost.m > 4 * 1024 * 1024 || cost.t > 64 || cost.p > 16 || cost.m < 8 {
        return Err(LockError::Damaged("bad header"));
    }
    let mut aad = first.to_string();
    let mut hint = None;
    let mut body = String::new();
    for l in it {
        if let Some(h) = l.strip_prefix("hint ") {
            aad.push('\n');
            aad.push_str(l);
            hint = B64.decode(h).ok().and_then(|b| String::from_utf8(b).ok());
        } else {
            body.push_str(l.trim());
        }
    }
    Ok(Header {
        salt: salt.ok_or(LockError::Damaged("bad header"))?,
        nonce: nonce.ok_or(LockError::Damaged("bad header"))?,
        cost,
        hint,
        aad,
        body: B64.decode(body).map_err(|_| LockError::Damaged("bad text"))?,
    })
}

/// Encrypts `plain` with `key` into the text of a locked note.
pub fn seal(plain: &str, key: &Key, hint: Option<&str>) -> String {
    let cipher = XChaCha20Poly1305::new(key.bytes.as_ref().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let mut aad = format!(
        "v1 argon2id m={} t={} p={} salt={} nonce={}",
        key.cost.m,
        key.cost.t,
        key.cost.p,
        B64.encode(key.salt),
        B64.encode(nonce)
    );
    if let Some(h) = hint.map(str::trim).filter(|h| !h.is_empty()) {
        aad.push_str("\nhint ");
        aad.push_str(&B64.encode(h));
    }
    let sealed = cipher
        .encrypt(&nonce, Payload { msg: plain.as_bytes(), aad: aad.as_bytes() })
        .expect("encrypting in memory can't fail");
    let body = B64.encode(sealed);
    let mut out = format!("{NOTICE}\n\n{FENCE}\n{aad}\n");
    for chunk in body.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

/// Decrypts a locked note's text with `key`.
pub fn open(text: &str, key: &Key) -> Result<String, LockError> {
    let h = header(text)?;
    if h.salt != key.salt || h.cost != key.cost {
        return Err(LockError::WrongPassword);
    }
    let cipher = XChaCha20Poly1305::new(key.bytes.as_ref().into());
    let plain = cipher
        .decrypt(XNonce::from_slice(&h.nonce), Payload { msg: &h.body, aad: h.aad.as_bytes() })
        .map_err(|_| LockError::WrongPassword)?;
    String::from_utf8(plain).map_err(|_| LockError::Damaged("bad text"))
}

/// Opens a locked note with a password (derives the key for its salt).
pub fn open_with_password(text: &str, password: &str) -> Result<(String, Key), LockError> {
    let h = header(text)?;
    let key = derive(password, h.salt, h.cost)?;
    let plain = open(text, &key)?;
    Ok((plain, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHEAP: Cost = Cost { m: 64, t: 1, p: 1 };

    fn key(pw: &str) -> Key {
        derive(pw, [7u8; 16], CHEAP).unwrap()
    }

    #[test]
    fn round_trip_with_hint() {
        let k = key("correct horse");
        let plain = "# Secret\n\nBank: [[Accounts]] #private\nÜnïcødé ✓\n";
        let locked = seal(plain, &k, Some("the horse"));
        assert!(is_locked(&locked));
        assert!(!locked.contains("Secret"));
        let h = header(&locked).unwrap();
        assert_eq!(h.hint.as_deref(), Some("the horse"));
        assert_eq!(h.salt_id(), k.salt_id());
        assert_eq!(open(&locked, &k).unwrap(), plain);
        let (again, k2) = open_with_password(&locked, "correct horse").unwrap();
        assert_eq!(again, plain);
        assert_eq!(k2.salt, k.salt);
    }

    #[test]
    fn wrong_password_and_tampering_are_caught() {
        let locked = seal("hello", &key("a"), Some("hint"));
        assert_eq!(open(&locked, &key("b")), Err(LockError::WrongPassword));
        // Changing the hint (it's authenticated) breaks it.
        let h = B64.encode("hint");
        let changed = locked.replace(&format!("hint {h}"), &format!("hint {}", B64.encode("other")));
        assert_eq!(open(&changed, &key("a")), Err(LockError::WrongPassword));
        // Flipping a ciphertext character breaks it.
        let mut lines: Vec<String> = locked.lines().map(String::from).collect();
        let i = lines.len() - 2;
        let c = if lines[i].starts_with('A') { "B" } else { "A" };
        lines[i].replace_range(0..1, c);
        assert!(open(&(lines.join("\n") + "\n"), &key("a")).is_err());
    }

    #[test]
    fn fresh_nonce_every_time() {
        let k = key("a");
        assert_ne!(seal("same", &k, None), seal("same", &k, None));
    }

    #[test]
    fn only_a_whole_locked_block_counts() {
        let k = key("a");
        let locked = seal("x", &k, None);
        assert!(is_locked(&locked));
        assert!(is_locked(locked.trim_end()));
        // A note that shows the syntax among other text isn't locked.
        assert!(!is_locked(&format!("# Notes\n\n{locked}")));
        assert!(!is_locked(&format!("{locked}\nmore text\n")));
        assert!(!is_locked("plain note"));
        assert_eq!(header("plain"), Err(LockError::NotLocked));
    }

    #[test]
    fn absurd_costs_are_refused() {
        let k = key("a");
        let locked = seal("x", &k, None).replace("m=64 ", "m=999999999 ");
        assert!(matches!(header(&locked), Err(LockError::Damaged(_))));
    }
}
