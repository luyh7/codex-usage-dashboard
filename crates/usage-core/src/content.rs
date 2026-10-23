//! Persisted chunk fingerprints. Background audits read a bounded amount per
//! tick; append parsing reuses complete chunks and verifies the old partial tail.
use crate::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub const CHUNK: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Fingerprint {
    pub len: u64,
    pub chunks: Vec<String>,
    pub digest: String,
}
impl Fingerprint {
    pub fn read(path: &Path, len: u64, previous: Option<&Self>) -> Result<Self, Error> {
        let mut file = File::open(path)?;
        let keep = previous.map_or(0, |old| (old.len / CHUNK) as usize);
        let mut chunks = previous.map_or_else(Vec::new, |old| old.chunks[..keep].to_vec());
        for offset in (keep as u64 * CHUNK..len).step_by(CHUNK as usize) {
            chunks.push(hash(&mut file, offset, (len - offset).min(CHUNK))?);
        }
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&(len, &chunks))?));
        Ok(Self {
            len,
            chunks,
            digest,
        })
    }
    pub fn matches_chunk(&self, path: &Path, index: usize) -> Result<bool, Error> {
        let offset = index as u64 * CHUNK;
        if offset >= self.len {
            return Ok(true);
        }
        Ok(hash(
            &mut File::open(path)?,
            offset,
            (self.len - offset).min(CHUNK),
        )? == self.chunks[index])
    }
    pub fn append_tail_matches(&self, path: &Path) -> bool {
        self.chunks.is_empty()
            || self
                .matches_chunk(path, self.chunks.len() - 1)
                .unwrap_or(false)
    }
}
fn hash(file: &mut File, offset: u64, len: u64) -> Result<String, Error> {
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; len as usize];
    file.read_exact(&mut bytes)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
