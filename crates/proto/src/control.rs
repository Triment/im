//! Control-plane messages. Frame layout on the wire:
//! `len:u32 | req_id:u32 | kind:u8 | body`, where `len` counts everything after itself.
//! `req_id == 0` marks a server push; responses echo the request id.

use crate::codec::{DecodeError, Reader, Result, Writer};

pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub const FILE_CHUNK_SIZE: u32 = 1024 * 1024;

pub mod err {
    pub const BAD_REQUEST: u16 = 400;
    pub const UNAUTHORIZED: u16 = 401;
    pub const FORBIDDEN: u16 = 403;
    pub const NOT_FOUND: u16 = 404;
    pub const CONFLICT: u16 = 409;
    pub const INTERNAL: u16 = 500;
}

pub mod sig {
    pub const INVITE: u8 = 1;
    pub const ACCEPT: u8 = 2;
    pub const REJECT: u8 = 3;
    pub const HANGUP: u8 = 4;
    /// Noise handshake message (opaque to the server).
    pub const NOISE: u8 = 5;
    /// ICE-lite style candidate list.
    pub const CANDIDATES: u8 = 6;
}

pub mod msg_kind {
    pub const TEXT: u8 = 0;
    pub const FILE: u8 = 1;
    pub const VOICE: u8 = 2;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friend {
    pub user_id: i64,
    pub username: String,
    pub display_name: String,
    /// 0 = request sent, 1 = request received, 2 = friends
    pub status: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub conv_id: i64,
    pub seq: i64,
    pub sender: i64,
    pub kind: u8,
    pub body: Vec<u8>,
    pub ts: i64,
    pub client_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvSummary {
    pub conv_id: i64,
    pub peer_id: i64,
    pub last_seq: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub pid: u32,
    pub user_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Register { username: String, password: String, display_name: String },
    Login { username: String, password: String, device_name: String },
    Resume { token: String },
    AuthOk { user_id: i64, token: String },
    Err { code: u16, message: String },
    Ok,
    Ping,
    Pong,

    FriendRequest { username: String },
    FriendAccept { user_id: i64 },
    FriendList,
    FriendListResp { friends: Vec<Friend> },

    SendMessage { to_user: i64, kind: u8, body: Vec<u8>, client_id: u64 },
    MessageAck { conv_id: i64, seq: i64, ts: i64 },
    Sync { conv_id: i64, after_seq: i64, limit: u32 },
    SyncResp { messages: Vec<StoredMessage> },
    ConvList,
    ConvListResp { convs: Vec<ConvSummary> },
    PushMessage(StoredMessage),
    Presence { user_id: i64, online: bool },

    FileInit { hash: [u8; 32], size: u64, name: String },
    FileInitResp { missing: Vec<u32>, chunk_size: u32 },
    FileChunk { hash: [u8; 32], index: u32, data: Vec<u8> },
    FileFinish { hash: [u8; 32] },
    FileGet { hash: [u8; 32], index: u32 },
    FileChunkResp { data: Vec<u8> },

    /// client -> server: `peer` = target. server -> client push: `peer` = sender.
    CallSignal { call_id: u64, peer: i64, kind: u8, data: Vec<u8> },
    MediaAlloc { call_id: u64 },
    MediaAllocResp { token: [u8; 16], udp_addr: String },

    RoomCreate,
    RoomCreated { room_id: u64 },
    RoomJoin { room_id: u64 },
    RoomJoined { pid: u32, token: [u8; 16], udp_addr: String, media_key: [u8; 32], participants: Vec<Participant> },
    RoomLeave { room_id: u64 },
    RoomEvent { room_id: u64, joined: bool, pid: u32, user_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub req_id: u32,
    pub msg: Msg,
}

impl StoredMessage {
    fn enc(&self, w: &mut Writer) {
        w.i64(self.conv_id).i64(self.seq).i64(self.sender).u8(self.kind).bytes(&self.body).i64(self.ts).u64(self.client_id);
    }
    fn dec(r: &mut Reader) -> Result<Self> {
        Ok(Self {
            conv_id: r.i64()?,
            seq: r.i64()?,
            sender: r.i64()?,
            kind: r.u8()?,
            body: r.bytes()?,
            ts: r.i64()?,
            client_id: r.u64()?,
        })
    }
}

impl Msg {
    pub fn kind(&self) -> u8 {
        use Msg::*;
        match self {
            Register { .. } => 1,
            Login { .. } => 2,
            Resume { .. } => 3,
            AuthOk { .. } => 4,
            Err { .. } => 5,
            Ok => 6,
            Ping => 7,
            Pong => 8,
            FriendRequest { .. } => 10,
            FriendAccept { .. } => 11,
            FriendList => 12,
            FriendListResp { .. } => 13,
            SendMessage { .. } => 14,
            MessageAck { .. } => 15,
            Sync { .. } => 16,
            SyncResp { .. } => 17,
            ConvList => 18,
            ConvListResp { .. } => 19,
            PushMessage(_) => 20,
            Presence { .. } => 21,
            FileInit { .. } => 30,
            FileInitResp { .. } => 31,
            FileChunk { .. } => 32,
            FileFinish { .. } => 33,
            FileGet { .. } => 34,
            FileChunkResp { .. } => 35,
            CallSignal { .. } => 40,
            MediaAlloc { .. } => 41,
            MediaAllocResp { .. } => 42,
            RoomCreate => 50,
            RoomCreated { .. } => 51,
            RoomJoin { .. } => 52,
            RoomJoined { .. } => 53,
            RoomLeave { .. } => 54,
            RoomEvent { .. } => 55,
        }
    }

    fn encode_body(&self, w: &mut Writer) {
        use Msg::*;
        match self {
            Register { username, password, display_name } => {
                w.str(username).str(password).str(display_name);
            }
            Login { username, password, device_name } => {
                w.str(username).str(password).str(device_name);
            }
            Resume { token } => {
                w.str(token);
            }
            AuthOk { user_id, token } => {
                w.i64(*user_id).str(token);
            }
            Err { code, message } => {
                w.u16(*code).str(message);
            }
            Ok | Ping | Pong | FriendList | ConvList | RoomCreate => {}
            FriendRequest { username } => {
                w.str(username);
            }
            FriendAccept { user_id } => {
                w.i64(*user_id);
            }
            FriendListResp { friends } => {
                w.vec(friends, |w, f| {
                    w.i64(f.user_id).str(&f.username).str(&f.display_name).u8(f.status);
                });
            }
            SendMessage { to_user, kind, body, client_id } => {
                w.i64(*to_user).u8(*kind).bytes(body).u64(*client_id);
            }
            MessageAck { conv_id, seq, ts } => {
                w.i64(*conv_id).i64(*seq).i64(*ts);
            }
            Sync { conv_id, after_seq, limit } => {
                w.i64(*conv_id).i64(*after_seq).u32(*limit);
            }
            SyncResp { messages } => {
                w.vec(messages, |w, m| m.enc(w));
            }
            ConvListResp { convs } => {
                w.vec(convs, |w, c| {
                    w.i64(c.conv_id).i64(c.peer_id).i64(c.last_seq);
                });
            }
            PushMessage(m) => m.enc(w),
            Presence { user_id, online } => {
                w.i64(*user_id).bool(*online);
            }
            FileInit { hash, size, name } => {
                w.raw(hash).u64(*size).str(name);
            }
            FileInitResp { missing, chunk_size } => {
                w.vec(missing, |w, i| {
                    w.u32(*i);
                })
                .u32(*chunk_size);
            }
            FileChunk { hash, index, data } => {
                w.raw(hash).u32(*index).bytes(data);
            }
            FileFinish { hash } => {
                w.raw(hash);
            }
            FileGet { hash, index } => {
                w.raw(hash).u32(*index);
            }
            FileChunkResp { data } => {
                w.bytes(data);
            }
            CallSignal { call_id, peer, kind, data } => {
                w.u64(*call_id).i64(*peer).u8(*kind).bytes(data);
            }
            MediaAlloc { call_id } => {
                w.u64(*call_id);
            }
            MediaAllocResp { token, udp_addr } => {
                w.raw(token).str(udp_addr);
            }
            RoomCreated { room_id } => {
                w.u64(*room_id);
            }
            RoomJoin { room_id } | RoomLeave { room_id } => {
                w.u64(*room_id);
            }
            RoomJoined { pid, token, udp_addr, media_key, participants } => {
                w.u32(*pid).raw(token).str(udp_addr).raw(media_key).vec(participants, |w, p| {
                    w.u32(p.pid).i64(p.user_id);
                });
            }
            RoomEvent { room_id, joined, pid, user_id } => {
                w.u64(*room_id).bool(*joined).u32(*pid).i64(*user_id);
            }
        }
    }

    fn decode_body(kind: u8, r: &mut Reader) -> Result<Msg> {
        use Msg::*;
        Result::Ok(match kind {
            1 => Register { username: r.str()?, password: r.str()?, display_name: r.str()? },
            2 => Login { username: r.str()?, password: r.str()?, device_name: r.str()? },
            3 => Resume { token: r.str()? },
            4 => AuthOk { user_id: r.i64()?, token: r.str()? },
            5 => Err { code: r.u16()?, message: r.str()? },
            6 => Ok,
            7 => Ping,
            8 => Pong,
            10 => FriendRequest { username: r.str()? },
            11 => FriendAccept { user_id: r.i64()? },
            12 => FriendList,
            13 => FriendListResp {
                friends: r.vec(|r| {
                    Result::Ok(Friend { user_id: r.i64()?, username: r.str()?, display_name: r.str()?, status: r.u8()? })
                })?,
            },
            14 => SendMessage { to_user: r.i64()?, kind: r.u8()?, body: r.bytes()?, client_id: r.u64()? },
            15 => MessageAck { conv_id: r.i64()?, seq: r.i64()?, ts: r.i64()? },
            16 => Sync { conv_id: r.i64()?, after_seq: r.i64()?, limit: r.u32()? },
            17 => SyncResp { messages: r.vec(StoredMessage::dec)? },
            18 => ConvList,
            19 => ConvListResp {
                convs: r.vec(|r| Result::Ok(ConvSummary { conv_id: r.i64()?, peer_id: r.i64()?, last_seq: r.i64()? }))?,
            },
            20 => PushMessage(StoredMessage::dec(r)?),
            21 => Presence { user_id: r.i64()?, online: r.bool()? },
            30 => FileInit { hash: r.array()?, size: r.u64()?, name: r.str()? },
            31 => FileInitResp { missing: r.vec(|r| r.u32())?, chunk_size: r.u32()? },
            32 => FileChunk { hash: r.array()?, index: r.u32()?, data: r.bytes()? },
            33 => FileFinish { hash: r.array()? },
            34 => FileGet { hash: r.array()?, index: r.u32()? },
            35 => FileChunkResp { data: r.bytes()? },
            40 => CallSignal { call_id: r.u64()?, peer: r.i64()?, kind: r.u8()?, data: r.bytes()? },
            41 => MediaAlloc { call_id: r.u64()? },
            42 => MediaAllocResp { token: r.array()?, udp_addr: r.str()? },
            50 => RoomCreate,
            51 => RoomCreated { room_id: r.u64()? },
            52 => RoomJoin { room_id: r.u64()? },
            53 => RoomJoined {
                pid: r.u32()?,
                token: r.array()?,
                udp_addr: r.str()?,
                media_key: r.array()?,
                participants: r.vec(|r| Result::Ok(Participant { pid: r.u32()?, user_id: r.i64()? }))?,
            },
            54 => RoomLeave { room_id: r.u64()? },
            55 => RoomEvent { room_id: r.u64()?, joined: r.bool()?, pid: r.u32()?, user_id: r.i64()? },
            k => return Result::Err(DecodeError::UnknownKind(k)),
        })
    }
}

impl Frame {
    pub fn new(req_id: u32, msg: Msg) -> Self {
        Self { req_id, msg }
    }

    /// Serialise including the 4-byte length prefix.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(0).u32(self.req_id).u8(self.msg.kind());
        self.msg.encode_body(&mut w);
        let len = (w.buf.len() - 4) as u32;
        w.buf[..4].copy_from_slice(&len.to_be_bytes());
        w.buf
    }

    /// Decode a frame body (everything after the length prefix).
    pub fn decode(body: &[u8]) -> Result<Frame> {
        if body.len() > MAX_FRAME {
            return Result::Err(DecodeError::TooLarge(body.len()));
        }
        let mut r = Reader::new(body);
        let req_id = r.u32()?;
        let kind = r.u8()?;
        let msg = Msg::decode_body(kind, &mut r)?;
        Result::Ok(Frame { req_id, msg })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(m: Msg) {
        let f = Frame::new(7, m);
        let enc = f.encode();
        let len = u32::from_be_bytes(enc[..4].try_into().unwrap()) as usize;
        assert_eq!(len, enc.len() - 4);
        assert_eq!(Frame::decode(&enc[4..]).unwrap(), f);
    }

    #[test]
    fn roundtrip_all() {
        rt(Msg::Login { username: "a".into(), password: "b".into(), device_name: "c".into() });
        rt(Msg::FriendListResp {
            friends: vec![Friend { user_id: 1, username: "u".into(), display_name: "U".into(), status: 2 }],
        });
        let m = StoredMessage { conv_id: 1, seq: 2, sender: 3, kind: 0, body: b"hi".to_vec(), ts: 99, client_id: 5 };
        rt(Msg::PushMessage(m.clone()));
        rt(Msg::SyncResp { messages: vec![m.clone(), m] });
        rt(Msg::FileChunk { hash: [9; 32], index: 3, data: vec![1; 1000] });
        rt(Msg::RoomJoined {
            pid: 4,
            token: [1; 16],
            udp_addr: "1.2.3.4:5".into(),
            media_key: [2; 32],
            participants: vec![Participant { pid: 1, user_id: 2 }],
        });
        rt(Msg::CallSignal { call_id: 1, peer: 2, kind: sig::NOISE, data: vec![0; 10] });
        rt(Msg::Ok);
        rt(Msg::Err { code: 404, message: "nope".into() });
    }

    #[test]
    fn rejects_garbage() {
        assert!(Frame::decode(&[0, 0, 0, 1, 250]).is_err());
        assert!(Frame::decode(&[0, 0]).is_err());
    }
}

/// Body of a `msg_kind::FILE` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRef {
    pub hash: [u8; 32],
    pub size: u64,
    pub name: String,
}

impl FileRef {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(&self.hash).u64(self.size).str(&self.name);
        w.buf
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut r = Reader::new(b);
        Ok(Self { hash: r.array()?, size: r.u64()?, name: r.str()? })
    }
}

/// ICE-lite candidate list carried in `sig::CANDIDATES`.
pub fn encode_candidates(c: &[String]) -> Vec<u8> {
    let mut w = Writer::new();
    w.vec(c, |w, s| {
        w.str(s);
    });
    w.buf
}

pub fn decode_candidates(b: &[u8]) -> Result<Vec<String>> {
    Reader::new(b).vec(|r| r.str())
}

#[cfg(test)]
mod ref_tests {
    use super::*;
    #[test]
    fn file_ref_and_candidates() {
        let f = FileRef { hash: [3; 32], size: 12345, name: "a.png".into() };
        assert_eq!(FileRef::decode(&f.encode()).unwrap(), f);
        let c = vec!["1.2.3.4:5".to_string(), "[::1]:9".to_string()];
        assert_eq!(decode_candidates(&encode_candidates(&c)).unwrap(), c);
    }
}
