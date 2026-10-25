//! Registry of live control connections for pushing messages to online users.

use std::{collections::HashMap, sync::Mutex};
use tokio::sync::mpsc::UnboundedSender;
use xmp_proto::control::{Frame, Msg};

#[derive(Default)]
pub struct Hub {
    sessions: Mutex<HashMap<i64, Vec<(u64, UnboundedSender<Frame>)>>>,
}

impl Hub {
    /// Returns true if this is the user's first session (they just came online).
    pub fn register(&self, user: i64, conn_id: u64, tx: UnboundedSender<Frame>) -> bool {
        let mut s = self.sessions.lock().unwrap();
        let v = s.entry(user).or_default();
        v.push((conn_id, tx));
        v.len() == 1
    }

    /// Returns true if the user has no sessions left (went offline).
    pub fn unregister(&self, user: i64, conn_id: u64) -> bool {
        let mut s = self.sessions.lock().unwrap();
        if let Some(v) = s.get_mut(&user) {
            v.retain(|(id, _)| *id != conn_id);
            if v.is_empty() {
                s.remove(&user);
                return true;
            }
        }
        false
    }

    pub fn is_online(&self, user: i64) -> bool {
        self.sessions.lock().unwrap().contains_key(&user)
    }

    /// Push to every session of `user`; returns how many sessions received it.
    pub fn push(&self, user: i64, msg: &Msg) -> usize {
        let s = self.sessions.lock().unwrap();
        let mut n = 0;
        if let Some(v) = s.get(&user) {
            for (_, tx) in v {
                if tx.send(Frame::new(0, msg.clone())).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }
}
