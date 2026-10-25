//! PostgreSQL access layer (tokio-postgres + deadpool).

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use deadpool_postgres::{ManagerConfig, Pool, RecyclingMethod};
use rand::RngCore;
use tokio_postgres::{error::SqlState, NoTls};
use xmp_proto::control::{err, ConvSummary, Friend, StoredMessage};

#[derive(Debug)]
pub struct ApiErr(pub u16, pub String);

impl ApiErr {
    pub fn new(code: u16, m: &str) -> Self {
        ApiErr(code, m.to_string())
    }
}
impl From<tokio_postgres::Error> for ApiErr {
    fn from(e: tokio_postgres::Error) -> Self {
        tracing::error!("db error: {e}");
        ApiErr(err::INTERNAL, "database error".into())
    }
}
impl From<deadpool_postgres::PoolError> for ApiErr {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        tracing::error!("pool error: {e}");
        ApiErr(err::INTERNAL, "database unavailable".into())
    }
}
pub type ApiResult<T> = Result<T, ApiErr>;

pub fn make_pool(conn: &str) -> anyhow::Result<Pool> {
    let cfg: tokio_postgres::Config = conn.parse()?;
    let mgr = deadpool_postgres::Manager::from_config(cfg, NoTls, ManagerConfig { recycling_method: RecyclingMethod::Fast });
    Ok(Pool::builder(mgr).max_size(32).build()?)
}

pub async fn migrate(pool: &Pool) -> anyhow::Result<()> {
    let c = pool.get().await?;
    // serialise concurrent migrations from several server instances
    c.batch_execute("SELECT pg_advisory_lock(7420001)").await?;
    let r = c.batch_execute(include_str!("../migrations/001_init.sql")).await;
    c.batch_execute("SELECT pg_advisory_unlock(7420001)").await?;
    r?;
    Ok(())
}

fn new_token() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    crate::files::hex(&b)
}

fn token_hash(t: &str) -> Vec<u8> {
    blake3::hash(t.as_bytes()).as_bytes().to_vec()
}

fn validate_username(u: &str) -> ApiResult<()> {
    let ok = (3..=32).contains(&u.len()) && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(ApiErr::new(err::BAD_REQUEST, "username must be 3-32 chars of [A-Za-z0-9_]"))
    }
}

pub async fn register(pool: &Pool, username: &str, password: &str, display_name: &str) -> ApiResult<i64> {
    validate_username(username)?;
    if password.len() < 8 {
        return Err(ApiErr::new(err::BAD_REQUEST, "password must be at least 8 characters"));
    }
    let pw = password.to_string();
    let hash = tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default().hash_password(pw.as_bytes(), &salt).map(|h| h.to_string())
    })
    .await
    .map_err(|_| ApiErr::new(err::INTERNAL, "hash task failed"))?
    .map_err(|_| ApiErr::new(err::INTERNAL, "hash failed"))?;
    let c = pool.get().await?;
    let name = if display_name.is_empty() { username } else { display_name };
    match c
        .query_one("INSERT INTO users(username, display_name, password_hash) VALUES ($1,$2,$3) RETURNING id", &[&username, &name, &hash])
        .await
    {
        Ok(r) => Ok(r.get(0)),
        Err(e) if e.code() == Some(&SqlState::UNIQUE_VIOLATION) => Err(ApiErr::new(err::CONFLICT, "username taken")),
        Err(e) => Err(e.into()),
    }
}

pub async fn login(pool: &Pool, username: &str, password: &str, device: &str) -> ApiResult<(i64, String)> {
    let c = pool.get().await?;
    let row = c.query_opt("SELECT id, password_hash FROM users WHERE username=$1", &[&username]).await?;
    let bad = || ApiErr::new(err::UNAUTHORIZED, "invalid username or password");
    let row = row.ok_or_else(bad)?;
    let (uid, hash): (i64, String) = (row.get(0), row.get(1));
    let pw = password.to_string();
    let ok = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok()).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    if !ok {
        return Err(bad());
    }
    let token = new_token();
    c.execute("INSERT INTO devices(user_id, name, token_hash) VALUES ($1,$2,$3)", &[&uid, &device, &token_hash(&token)]).await?;
    Ok((uid, token))
}

pub async fn resume(pool: &Pool, token: &str) -> ApiResult<i64> {
    let c = pool.get().await?;
    let r = c
        .query_opt(
            "UPDATE devices SET last_seen_at=(extract(epoch from now())*1000)::bigint WHERE token_hash=$1 RETURNING user_id",
            &[&token_hash(token)],
        )
        .await?;
    r.map(|r| r.get(0)).ok_or_else(|| ApiErr::new(err::UNAUTHORIZED, "invalid token"))
}

pub async fn user_id_by_name(pool: &Pool, username: &str) -> ApiResult<i64> {
    let c = pool.get().await?;
    let r = c.query_opt("SELECT id FROM users WHERE username=$1", &[&username]).await?;
    r.map(|r| r.get(0)).ok_or_else(|| ApiErr::new(err::NOT_FOUND, "no such user"))
}

pub async fn friend_request(pool: &Pool, me: i64, username: &str) -> ApiResult<()> {
    let target = user_id_by_name(pool, username).await?;
    if target == me {
        return Err(ApiErr::new(err::BAD_REQUEST, "cannot add yourself"));
    }
    let mut c = pool.get().await?;
    let tx = c.transaction().await?;
    if tx.query_opt("SELECT 1 FROM friendships WHERE user_id=$1 AND friend_id=$2", &[&me, &target]).await?.is_some() {
        return Ok(()); // idempotent
    }
    let reverse = tx.query_opt("SELECT 1 FROM friendships WHERE user_id=$1 AND friend_id=$2 AND status=0", &[&target, &me]).await?;
    if reverse.is_some() {
        tx.execute("UPDATE friendships SET status=1 WHERE user_id=$1 AND friend_id=$2", &[&target, &me]).await?;
        tx.execute("INSERT INTO friendships(user_id, friend_id, status) VALUES ($1,$2,1)", &[&me, &target]).await?;
    } else {
        tx.execute("INSERT INTO friendships(user_id, friend_id, status) VALUES ($1,$2,0)", &[&me, &target]).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn friend_accept(pool: &Pool, me: i64, other: i64) -> ApiResult<()> {
    let mut c = pool.get().await?;
    let tx = c.transaction().await?;
    let n = tx.execute("UPDATE friendships SET status=1 WHERE user_id=$1 AND friend_id=$2 AND status=0", &[&other, &me]).await?;
    if n == 0 {
        return Err(ApiErr::new(err::NOT_FOUND, "no pending request"));
    }
    tx.execute(
        "INSERT INTO friendships(user_id, friend_id, status) VALUES ($1,$2,1) ON CONFLICT (user_id, friend_id) DO UPDATE SET status=1",
        &[&me, &other],
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn friend_list(pool: &Pool, me: i64) -> ApiResult<Vec<Friend>> {
    let c = pool.get().await?;
    let rows = c
        .query(
            "SELECT u.id, u.username, u.display_name,
                    (CASE WHEN f.user_id=$1 THEN (CASE WHEN f.status=1 THEN 2 ELSE 0 END) ELSE 1 END)::int
             FROM friendships f
             JOIN users u ON u.id = (CASE WHEN f.user_id=$1 THEN f.friend_id ELSE f.user_id END)
             WHERE f.user_id=$1 OR (f.friend_id=$1 AND f.status=0)
             ORDER BY u.username",
            &[&me],
        )
        .await?;
    Ok(rows.iter().map(|r| Friend { user_id: r.get(0), username: r.get(1), display_name: r.get(2), status: r.get::<_, i32>(3) as u8 }).collect())
}

pub async fn friend_ids(pool: &Pool, me: i64) -> ApiResult<Vec<i64>> {
    let c = pool.get().await?;
    let rows = c.query("SELECT friend_id FROM friendships WHERE user_id=$1 AND status=1", &[&me]).await?;
    Ok(rows.iter().map(|r| r.get(0)).collect())
}

pub async fn are_friends(pool: &Pool, a: i64, b: i64) -> ApiResult<bool> {
    let c = pool.get().await?;
    Ok(c.query_opt("SELECT 1 FROM friendships WHERE user_id=$1 AND friend_id=$2 AND status=1", &[&a, &b]).await?.is_some())
}

pub async fn send_message(pool: &Pool, sender: i64, to: i64, kind: u8, body: &[u8], client_id: u64) -> ApiResult<StoredMessage> {
    if !are_friends(pool, sender, to).await? {
        return Err(ApiErr::new(err::FORBIDDEN, "not friends"));
    }
    let (a, b) = (sender.min(to), sender.max(to));
    let cid = client_id as i64;
    let mut c = pool.get().await?;
    let tx = c.transaction().await?;
    let conv: i64 = tx
        .query_one(
            "INSERT INTO conversations(user_a, user_b) VALUES ($1,$2)
             ON CONFLICT (user_a, user_b) DO UPDATE SET user_a = EXCLUDED.user_a RETURNING id",
            &[&a, &b],
        )
        .await?
        .get(0);
    if let Some(r) = tx.query_opt("SELECT conv_id, seq FROM message_dedup WHERE sender=$1 AND client_id=$2", &[&sender, &cid]).await? {
        let (conv_id, seq): (i64, i64) = (r.get(0), r.get(1));
        let m = tx
            .query_one("SELECT kind, body, created_at FROM messages WHERE conv_id=$1 AND seq=$2", &[&conv_id, &seq])
            .await?;
        return Ok(StoredMessage { conv_id, seq, sender, kind: m.get::<_, i16>(0) as u8, body: m.get(1), ts: m.get(2), client_id });
    }
    // The UPDATE takes a row lock on the conversation, serialising seq allocation.
    let seq: i64 = tx.query_one("UPDATE conversations SET last_seq = last_seq + 1 WHERE id=$1 RETURNING last_seq", &[&conv]).await?.get(0);
    let ts: i64 = tx
        .query_one(
            "INSERT INTO messages(conv_id, seq, sender, kind, body, client_id) VALUES ($1,$2,$3,$4,$5,$6) RETURNING created_at",
            &[&conv, &seq, &sender, &(kind as i16), &body, &cid],
        )
        .await?
        .get(0);
    tx.execute("INSERT INTO message_dedup(sender, client_id, conv_id, seq) VALUES ($1,$2,$3,$4)", &[&sender, &cid, &conv, &seq]).await?;
    tx.commit().await?;
    Ok(StoredMessage { conv_id: conv, seq, sender, kind, body: body.to_vec(), ts, client_id })
}

pub async fn sync(pool: &Pool, me: i64, conv_id: i64, after: i64, limit: u32) -> ApiResult<Vec<StoredMessage>> {
    let c = pool.get().await?;
    let member = c.query_opt("SELECT 1 FROM conversations WHERE id=$1 AND (user_a=$2 OR user_b=$2)", &[&conv_id, &me]).await?;
    if member.is_none() {
        return Err(ApiErr::new(err::FORBIDDEN, "not a member of this conversation"));
    }
    let rows = c
        .query(
            "SELECT seq, sender, kind, body, created_at, client_id FROM messages WHERE conv_id=$1 AND seq>$2 ORDER BY seq LIMIT $3",
            &[&conv_id, &after, &(limit.clamp(1, 500) as i64)],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| StoredMessage {
            conv_id,
            seq: r.get(0),
            sender: r.get(1),
            kind: r.get::<_, i16>(2) as u8,
            body: r.get(3),
            ts: r.get(4),
            client_id: r.get::<_, i64>(5) as u64,
        })
        .collect())
}

pub async fn conv_list(pool: &Pool, me: i64) -> ApiResult<Vec<ConvSummary>> {
    let c = pool.get().await?;
    let rows = c
        .query(
            "SELECT id, (CASE WHEN user_a=$1 THEN user_b ELSE user_a END), last_seq FROM conversations WHERE user_a=$1 OR user_b=$1 ORDER BY id",
            &[&me],
        )
        .await?;
    Ok(rows.iter().map(|r| ConvSummary { conv_id: r.get(0), peer_id: r.get(1), last_seq: r.get(2) }).collect())
}

pub async fn call_started(pool: &Pool, call_id: u64, kind: &str, by: i64) {
    if let Ok(c) = pool.get().await {
        let _ = c
            .execute("INSERT INTO calls(id, kind, started_by) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING", &[&(call_id as i64), &kind, &by])
            .await;
    }
}

pub async fn call_ended(pool: &Pool, call_id: u64) {
    if let Ok(c) = pool.get().await {
        let _ = c
            .execute(
                "UPDATE calls SET ended_at=(extract(epoch from now())*1000)::bigint WHERE id=$1 AND ended_at IS NULL",
                &[&(call_id as i64)],
            )
            .await;
    }
}
