//! Content-addressed chunk store. Layout: `<root>/<hex(hash)>/<index>`.
//! Files are identified by the blake3 hash of their full content, so identical
//! uploads are de-duplicated and interrupted uploads resume by asking which chunks are missing.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use xmp_proto::control::FILE_CHUNK_SIZE;

pub struct BlobStore {
    root: PathBuf,
}

#[derive(Debug)]
pub enum FileError {
    BadRequest(&'static str),
    NotFound,
    Io(std::io::Error),
}

impl From<std::io::Error> for FileError {
    fn from(e: std::io::Error) -> Self {
        FileError::Io(e)
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn chunk_count(size: u64) -> u32 {
    size.div_ceil(FILE_CHUNK_SIZE as u64).max(1) as u32
}

fn expected_len(size: u64, index: u32) -> u64 {
    let start = index as u64 * FILE_CHUNK_SIZE as u64;
    (size - start.min(size)).min(FILE_CHUNK_SIZE as u64)
}

impl BlobStore {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn dir(&self, hash: &[u8; 32]) -> PathBuf {
        self.root.join(hex(hash))
    }

    fn size_file(dir: &Path) -> PathBuf {
        dir.join("size")
    }

    fn read_size(&self, hash: &[u8; 32]) -> Result<u64, FileError> {
        let s = fs::read_to_string(Self::size_file(&self.dir(hash))).map_err(|_| FileError::NotFound)?;
        s.trim().parse().map_err(|_| FileError::NotFound)
    }

    pub fn is_complete(&self, hash: &[u8; 32]) -> bool {
        self.dir(hash).join("complete").exists()
    }

    /// Register an upload; returns the chunk indexes the server still needs.
    pub fn init(&self, hash: &[u8; 32], size: u64) -> Result<Vec<u32>, FileError> {
        if size > 64 * 1024 * 1024 * 1024 {
            return Err(FileError::BadRequest("file too large"));
        }
        let dir = self.dir(hash);
        fs::create_dir_all(&dir)?;
        if let Ok(existing) = self.read_size(hash) {
            if existing != size {
                return Err(FileError::BadRequest("size mismatch with existing upload"));
            }
        } else {
            fs::write(Self::size_file(&dir), size.to_string())?;
        }
        if self.is_complete(hash) {
            return Ok(vec![]);
        }
        Ok((0..chunk_count(size)).filter(|i| !dir.join(i.to_string()).exists()).collect())
    }

    pub fn put_chunk(&self, hash: &[u8; 32], index: u32, data: &[u8]) -> Result<(), FileError> {
        let size = self.read_size(hash)?;
        if index >= chunk_count(size) {
            return Err(FileError::BadRequest("chunk index out of range"));
        }
        if data.len() as u64 != expected_len(size, index) {
            return Err(FileError::BadRequest("wrong chunk length"));
        }
        let dir = self.dir(hash);
        let tmp = dir.join(format!("{index}.tmp"));
        fs::write(&tmp, data)?;
        fs::rename(tmp, dir.join(index.to_string()))?;
        Ok(())
    }

    /// Verify the assembled content against the claimed hash and mark the file complete.
    pub fn finish(&self, hash: &[u8; 32]) -> Result<(), FileError> {
        let size = self.read_size(hash)?;
        let dir = self.dir(hash);
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; 64 * 1024];
        for i in 0..chunk_count(size) {
            let mut f = fs::File::open(dir.join(i.to_string())).map_err(|_| FileError::BadRequest("missing chunk"))?;
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
        }
        if hasher.finalize().as_bytes() != hash {
            // corrupted upload: drop chunks so the client restarts cleanly
            for i in 0..chunk_count(size) {
                let _ = fs::remove_file(dir.join(i.to_string()));
            }
            return Err(FileError::BadRequest("hash mismatch"));
        }
        fs::write(dir.join("complete"), b"")?;
        Ok(())
    }

    pub fn get_chunk(&self, hash: &[u8; 32], index: u32) -> Result<Vec<u8>, FileError> {
        if !self.is_complete(hash) {
            return Err(FileError::NotFound);
        }
        fs::read(self.dir(hash).join(index.to_string())).map_err(|_| FileError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_resume_and_verify() {
        let dir = std::env::temp_dir().join(format!("xmp-blob-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = BlobStore::new(dir.clone()).unwrap();
        let data: Vec<u8> = (0..(FILE_CHUNK_SIZE as usize * 2 + 100)).map(|i| (i % 251) as u8).collect();
        let hash = *blake3::hash(&data).as_bytes();
        assert_eq!(s.init(&hash, data.len() as u64).unwrap(), vec![0, 1, 2]);
        let cs = FILE_CHUNK_SIZE as usize;
        s.put_chunk(&hash, 0, &data[..cs]).unwrap();
        assert!(s.put_chunk(&hash, 1, &data[..10]).is_err());
        // "resume": only chunks 1,2 missing
        assert_eq!(s.init(&hash, data.len() as u64).unwrap(), vec![1, 2]);
        s.put_chunk(&hash, 1, &data[cs..2 * cs]).unwrap();
        s.put_chunk(&hash, 2, &data[2 * cs..]).unwrap();
        assert!(s.get_chunk(&hash, 0).is_err()); // not complete yet
        s.finish(&hash).unwrap();
        assert_eq!(s.get_chunk(&hash, 2).unwrap(), &data[2 * cs..]);
        assert!(s.init(&hash, data.len() as u64).unwrap().is_empty()); // dedup
        // corrupted upload is rejected
        let bad_hash = [9u8; 32];
        s.init(&bad_hash, 10).unwrap();
        s.put_chunk(&bad_hash, 0, &[0; 10]).unwrap();
        assert!(s.finish(&bad_hash).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
