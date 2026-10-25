//! Chunked, resumable, hash-verified file transfer through the server blob store.

use crate::{other, Client, Error, Result};
use std::{io::SeekFrom, path::Path, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Semaphore,
};
use xmp_proto::control::{msg_kind, FileRef, Msg, FILE_CHUNK_SIZE};

const PARALLEL_CHUNKS: usize = 4;

fn chunk_count(size: u64) -> u32 {
    size.div_ceil(FILE_CHUNK_SIZE as u64).max(1) as u32
}

pub async fn hash_file(path: &Path) -> Result<([u8; 32], u64)> {
    let mut f = tokio::fs::File::open(path).await?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
    }
    Ok((*h.finalize().as_bytes(), total))
}

impl Client {
    /// Upload a file (skipping chunks the server already has) and return its reference.
    pub async fn upload_file(&self, path: &Path, progress: impl Fn(u32, u32) + Send + Sync + 'static) -> Result<FileRef> {
        let (hash, size) = hash_file(path).await?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let (missing, chunk_size) = match self.request(Msg::FileInit { hash, size, name: name.clone() }).await? {
            Msg::FileInitResp { missing, chunk_size } => (missing, chunk_size),
            m => return Err(other(format!("unexpected {m:?}"))),
        };
        let total = missing.len() as u32;
        let sem = Arc::new(Semaphore::new(PARALLEL_CHUNKS));
        let progress = Arc::new(progress);
        let done = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut tasks = tokio::task::JoinSet::new();
        for idx in missing {
            let permit = sem.clone().acquire_owned().await.map_err(other)?;
            let (c, path, progress, done) = (self.clone(), path.to_path_buf(), progress.clone(), done.clone());
            tasks.spawn(async move {
                let _permit = permit;
                let mut f = tokio::fs::File::open(&path).await?;
                let off = idx as u64 * chunk_size as u64;
                f.seek(SeekFrom::Start(off)).await?;
                let len = (size - off).min(chunk_size as u64) as usize;
                let mut data = vec![0u8; len];
                f.read_exact(&mut data).await?;
                c.request(Msg::FileChunk { hash, index: idx, data }).await?;
                progress(done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1, total);
                Ok::<_, Error>(())
            });
        }
        while let Some(r) = tasks.join_next().await {
            r.map_err(other)??;
        }
        match self.request(Msg::FileFinish { hash }).await? {
            Msg::Ok => {}
            m => return Err(other(format!("unexpected {m:?}"))),
        }
        Ok(FileRef { hash, size, name })
    }

    /// Upload then send as a FILE message.
    pub async fn send_file(&self, to_user: i64, path: &Path) -> Result<FileRef> {
        let fr = self.upload_file(path, |_, _| {}).await?;
        self.send_message(to_user, msg_kind::FILE, fr.encode(), crate::rand_id()).await?;
        Ok(fr)
    }

    /// Download into `dest`, verifying the blake3 hash. Resumes from an existing `.part` file.
    pub async fn download_file(&self, fr: &FileRef, dest: &Path) -> Result<()> {
        let part = dest.with_extension("part");
        let total = chunk_count(fr.size);
        let have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
        let mut next = (have / FILE_CHUNK_SIZE as u64) as u32; // only whole chunks count
        let mut f = tokio::fs::OpenOptions::new().create(true).write(true).open(&part).await?;
        f.set_len(next as u64 * FILE_CHUNK_SIZE as u64).await?;
        f.seek(SeekFrom::Start(next as u64 * FILE_CHUNK_SIZE as u64)).await?;
        while next < total {
            match self.request(Msg::FileGet { hash: fr.hash, index: next }).await? {
                Msg::FileChunkResp { data } => f.write_all(&data).await?,
                m => return Err(other(format!("unexpected {m:?}"))),
            }
            next += 1;
        }
        f.flush().await?;
        drop(f);
        let (hash, size) = hash_file(&part).await?;
        if hash != fr.hash || size != fr.size {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::Other("downloaded file failed hash verification".into()));
        }
        tokio::fs::rename(&part, dest).await?;
        Ok(())
    }
}
