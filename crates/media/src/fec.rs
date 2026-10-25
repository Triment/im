//! Reed-Solomon FEC over groups of consecutive media packets.
//!
//! Sender: data packets carry `fec = k<<4 | idx`; after `k` packets, `m` parity
//! packets are emitted (flag PARITY, `seq = group_start`, `fec = k<<4 | parity_idx`).
//! Parity payload = `[m][shard]`. Shards are `len:u16 | payload | zero pad`.
//! FEC runs over plaintext payloads, each packet is then encrypted individually.

use reed_solomon_erasure::galois_8::ReedSolomon;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parity {
    pub group_start: u32,
    pub fec: u8,
    pub payload: Vec<u8>,
}

pub struct FecEncoder {
    next_k: usize,
    next_m: usize,
    k: usize,
    m: usize,
    group_start: u32,
    shards: Vec<Vec<u8>>,
}

fn shardify(p: &[u8], len: usize) -> Vec<u8> {
    let mut s = Vec::with_capacity(len);
    s.extend_from_slice(&(p.len() as u16).to_be_bytes());
    s.extend_from_slice(p);
    s.resize(len, 0);
    s
}

impl FecEncoder {
    /// `k` data packets per group (<=15), `m` parity packets (<=15). `k == 0 || m == 0` disables FEC.
    pub fn new(k: usize, m: usize) -> Self {
        let mut e = Self { next_k: 0, next_m: 0, k: 0, m: 0, group_start: 0, shards: vec![] };
        e.set_params(k, m);
        e
    }

    /// Takes effect at the next group boundary.
    pub fn set_params(&mut self, k: usize, m: usize) {
        if k == 0 || m == 0 {
            self.next_k = 0;
            self.next_m = 0;
        } else {
            self.next_k = k.min(15);
            self.next_m = m.min(15);
        }
    }

    /// Register a data packet. Returns the `fec` header byte to use for it and any parity
    /// packets that are now due.
    pub fn push(&mut self, seq: u32, payload: &[u8]) -> (u8, Vec<Parity>) {
        // groups must be made of consecutive seqs
        if !self.shards.is_empty() && seq != self.group_start + self.shards.len() as u32 {
            self.shards.clear();
        }
        if self.shards.is_empty() {
            self.k = self.next_k;
            self.m = self.next_m;
            self.group_start = seq;
        }
        if self.k == 0 {
            return (0, vec![]);
        }
        let idx = self.shards.len();
        let fec = ((self.k as u8) << 4) | idx as u8;
        self.shards.push(payload.to_vec());
        if self.shards.len() < self.k {
            return (fec, vec![]);
        }
        let shard_len = self.shards.iter().map(|s| s.len() + 2).max().unwrap();
        let mut all: Vec<Vec<u8>> = self.shards.iter().map(|s| shardify(s, shard_len)).collect();
        all.resize(self.k + self.m, vec![0; shard_len]);
        let rs = ReedSolomon::new(self.k, self.m).expect("valid rs params");
        rs.encode(&mut all).expect("rs encode");
        let parity = (0..self.m)
            .map(|i| {
                let mut p = vec![self.m as u8];
                p.extend_from_slice(&all[self.k + i]);
                Parity { group_start: self.group_start, fec: ((self.k as u8) << 4) | i as u8, payload: p }
            })
            .collect();
        self.shards.clear();
        (fec, parity)
    }
}

struct Group {
    k: usize,
    m: usize,
    data: Vec<Option<Vec<u8>>>,
    parity: Vec<Option<Vec<u8>>>,
    done: bool,
}

#[derive(Default)]
pub struct FecDecoder {
    groups: BTreeMap<u32, Group>,
}

impl FecDecoder {
    const MAX_GROUPS: usize = 64;

    pub fn new() -> Self {
        Self::default()
    }

    fn group(&mut self, start: u32, k: usize) -> &mut Group {
        while self.groups.len() >= Self::MAX_GROUPS && !self.groups.contains_key(&start) {
            let first = *self.groups.keys().next().unwrap();
            self.groups.remove(&first);
        }
        self.groups.entry(start).or_insert_with(|| Group { k, m: 0, data: vec![None; k], parity: vec![], done: false })
    }

    /// Feed a received data packet; returns payloads recovered by this arrival as `(seq, payload)`.
    pub fn on_data(&mut self, seq: u32, fec: u8, payload: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let (k, idx) = ((fec >> 4) as usize, (fec & 0x0f) as usize);
        if k == 0 || idx >= k || (seq as usize) < idx {
            return vec![];
        }
        let start = seq - idx as u32;
        let g = self.group(start, k);
        if g.k != k {
            return vec![];
        }
        g.data[idx] = Some(payload.to_vec());
        Self::try_recover(start, g)
    }

    pub fn on_parity(&mut self, group_start: u32, fec: u8, payload: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let (k, pidx) = ((fec >> 4) as usize, (fec & 0x0f) as usize);
        if k == 0 || payload.len() < 3 {
            return vec![];
        }
        let m = payload[0] as usize;
        if m == 0 || pidx >= m {
            return vec![];
        }
        let g = self.group(group_start, k);
        if g.k != k || (g.m != 0 && g.m != m) {
            return vec![];
        }
        if g.parity.len() != m {
            g.m = m;
            g.parity = vec![None; m];
        }
        g.parity[pidx] = Some(payload[1..].to_vec());
        Self::try_recover(group_start, g)
    }

    fn try_recover(start: u32, g: &mut Group) -> Vec<(u32, Vec<u8>)> {
        if g.done || g.m == 0 {
            return vec![];
        }
        let have_data = g.data.iter().filter(|d| d.is_some()).count();
        if have_data == g.k {
            g.done = true;
            return vec![];
        }
        let have_parity = g.parity.iter().filter(|p| p.is_some()).count();
        if have_data + have_parity < g.k {
            return vec![];
        }
        let shard_len = g.parity.iter().flatten().map(|p| p.len()).next().unwrap();
        let mut shards: Vec<Option<Vec<u8>>> = Vec::with_capacity(g.k + g.m);
        for d in &g.data {
            shards.push(match d {
                Some(p) if p.len() + 2 <= shard_len => Some(shardify(p, shard_len)),
                _ => None,
            });
        }
        for p in &g.parity {
            shards.push(p.clone().filter(|p| p.len() == shard_len));
        }
        let Ok(rs) = ReedSolomon::new(g.k, g.m) else { return vec![] };
        if rs.reconstruct_data(&mut shards).is_err() {
            return vec![];
        }
        g.done = true;
        let mut out = vec![];
        for i in 0..g.k {
            if g.data[i].is_none() {
                if let Some(s) = &shards[i] {
                    let len = u16::from_be_bytes([s[0], s[1]]) as usize;
                    if 2 + len <= s.len() {
                        out.push((start + i as u32, s[2..2 + len].to_vec()));
                    }
                }
            }
        }
        out
    }
}

/// Choose `(k, m)` for the observed loss fraction (0.0..1.0). `(0, 0)` = FEC off.
pub fn adaptive_params(loss: f32) -> (usize, usize) {
    if loss < 0.005 {
        (0, 0)
    } else if loss < 0.02 {
        (10, 1)
    } else if loss < 0.05 {
        (8, 2)
    } else if loss < 0.12 {
        (6, 3)
    } else {
        (5, 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(n: usize, k: usize, m: usize) -> (Vec<(u32, u8, Vec<u8>)>, Vec<Parity>) {
        let mut enc = FecEncoder::new(k, m);
        let mut data = vec![];
        let mut par = vec![];
        for i in 0..n {
            let payload: Vec<u8> = (0..(50 + i * 13)).map(|x| (x + i) as u8).collect();
            let (fec, p) = enc.push(100 + i as u32, &payload);
            data.push((100 + i as u32, fec, payload));
            par.extend(p);
        }
        (data, par)
    }

    #[test]
    fn recovers_up_to_m_losses() {
        let (data, par) = make(8, 8, 2);
        assert_eq!(par.len(), 2);
        let mut dec = FecDecoder::new();
        let mut recovered = vec![];
        for (i, (seq, fec, p)) in data.iter().enumerate() {
            if i == 2 || i == 5 {
                continue; // lost
            }
            recovered.extend(dec.on_data(*seq, *fec, p));
        }
        for p in &par {
            recovered.extend(dec.on_parity(p.group_start, p.fec, &p.payload));
        }
        recovered.sort();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0], (102, data[2].2.clone()));
        assert_eq!(recovered[1], (105, data[5].2.clone()));
    }

    #[test]
    fn too_many_losses_not_recovered() {
        let (data, par) = make(6, 6, 1);
        let mut dec = FecDecoder::new();
        let mut rec = vec![];
        for (i, (s, f, p)) in data.iter().enumerate() {
            if i < 2 {
                continue;
            }
            rec.extend(dec.on_data(*s, *f, p));
        }
        for p in &par {
            rec.extend(dec.on_parity(p.group_start, p.fec, &p.payload));
        }
        assert!(rec.is_empty());
    }

    #[test]
    fn parity_first_then_data() {
        let (data, par) = make(4, 4, 1);
        let mut dec = FecDecoder::new();
        let mut rec = vec![];
        rec.extend(dec.on_parity(par[0].group_start, par[0].fec, &par[0].payload));
        for (i, (s, f, p)) in data.iter().enumerate() {
            if i != 1 {
                rec.extend(dec.on_data(*s, *f, p));
            }
        }
        assert_eq!(rec, vec![(101, data[1].2.clone())]);
    }

    #[test]
    fn fec_off() {
        let mut e = FecEncoder::new(0, 0);
        assert_eq!(e.push(1, b"x"), (0, vec![]));
    }
}
