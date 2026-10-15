//! Embeddings of note chunks for search by meaning, keyed by the chunk's
//! content hash so unchanged text is never sent to the model again. Kept in
//! the app's cache folder (never in the vault) in a small binary file.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 6] = b"MRWV1\n";

#[derive(Debug, Default, Clone)]
pub struct VectorStore {
    /// Which model made these ("provider-id/model"); a different one starts over.
    pub model: String,
    pub dim: usize,
    map: HashMap<u64, Vec<f32>>,
}

fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

impl VectorStore {
    pub fn new(model: &str) -> Self {
        VectorStore { model: model.to_string(), dim: 0, map: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn has(&self, hash: u64) -> bool {
        self.map.contains_key(&hash)
    }

    pub fn insert(&mut self, hash: u64, mut v: Vec<f32>) {
        if v.is_empty() {
            return;
        }
        if self.dim == 0 {
            self.dim = v.len();
        }
        if v.len() != self.dim {
            return;
        }
        normalize(&mut v);
        self.map.insert(hash, v);
    }

    /// Drops vectors for text that's gone.
    pub fn retain(&mut self, live: &HashSet<u64>) {
        self.map.retain(|h, _| live.contains(h));
    }

    /// Cosine similarity of `query` to each hash; (index into `hashes`, score), best first.
    pub fn search(&self, hashes: &[u64], query: &[f32], k: usize) -> Vec<(usize, f32)> {
        if query.len() != self.dim || self.dim == 0 {
            return Vec::new();
        }
        let mut q = query.to_vec();
        normalize(&mut q);
        let mut out: Vec<(usize, f32)> = hashes
            .iter()
            .enumerate()
            .filter_map(|(i, h)| self.map.get(h).map(|v| (i, v.iter().zip(&q).map(|(a, b)| a * b).sum::<f32>())))
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out.truncate(k);
        out
    }

    pub fn load(path: &Path) -> Option<Self> {
        let mut f = fs::File::open(path).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        let mut r = &buf[..];
        let mut take = |n: usize| -> Option<&[u8]> {
            if r.len() < n {
                return None;
            }
            let (a, b) = r.split_at(n);
            r = b;
            Some(a)
        };
        if take(6)? != MAGIC {
            return None;
        }
        let ml = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        let model = String::from_utf8(take(ml)?.to_vec()).ok()?;
        let dim = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        let count = u64::from_le_bytes(take(8)?.try_into().ok()?) as usize;
        let mut map = HashMap::with_capacity(count);
        for _ in 0..count {
            let h = u64::from_le_bytes(take(8)?.try_into().ok()?);
            let raw = take(dim * 4)?;
            let v: Vec<f32> = raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            map.insert(h, v);
        }
        Some(VectorStore { model, dim, map })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        {
            let mut f = io::BufWriter::new(fs::File::create(&tmp)?);
            f.write_all(MAGIC)?;
            f.write_all(&(self.model.len() as u32).to_le_bytes())?;
            f.write_all(self.model.as_bytes())?;
            f.write_all(&(self.dim as u32).to_le_bytes())?;
            f.write_all(&(self.map.len() as u64).to_le_bytes())?;
            for (h, v) in &self.map {
                f.write_all(&h.to_le_bytes())?;
                for x in v {
                    f.write_all(&x.to_le_bytes())?;
                }
            }
            f.flush()?;
        }
        fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_loads_and_ranks() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("v.bin");
        let mut s = VectorStore::new("ollama/nomic");
        s.insert(1, vec![1.0, 0.0, 0.0]);
        s.insert(2, vec![0.0, 1.0, 0.0]);
        s.insert(3, vec![0.7, 0.7, 0.0]);
        s.insert(4, vec![1.0, 0.0]); // wrong size: ignored
        s.save(&p).unwrap();
        let t = VectorStore::load(&p).unwrap();
        assert_eq!(t.model, "ollama/nomic");
        assert_eq!(t.len(), 3);
        let r = t.search(&[1, 2, 3, 9], &[2.0, 0.1, 0.0], 2);
        assert_eq!(r[0].0, 0);
        assert_eq!(r[1].0, 2);
        let mut t = t;
        t.retain(&[2u64].into_iter().collect());
        assert_eq!(t.len(), 1);
    }
}
