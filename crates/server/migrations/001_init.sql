-- XMP schema. All timestamps are unix epoch milliseconds (BIGINT) to keep the wire format trivial.

CREATE TABLE IF NOT EXISTS users (
    id            BIGSERIAL PRIMARY KEY,
    username      TEXT   NOT NULL UNIQUE,
    display_name  TEXT   NOT NULL,
    password_hash TEXT   NOT NULL,
    created_at    BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint
);

-- One row per logged-in device; token_hash = blake3(token).
CREATE TABLE IF NOT EXISTS devices (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT   NOT NULL,
    token_hash   BYTEA  NOT NULL UNIQUE,
    created_at   BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    last_seen_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint
);
CREATE INDEX IF NOT EXISTS devices_user_idx ON devices(user_id);

-- Directed request rows; a friendship exists when both directions are 'accepted'.
CREATE TABLE IF NOT EXISTS friendships (
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    friend_id  BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status     SMALLINT NOT NULL,           -- 0 pending (user_id asked friend_id), 1 accepted
    created_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    PRIMARY KEY (user_id, friend_id),
    CHECK (user_id <> friend_id)
);
CREATE INDEX IF NOT EXISTS friendships_friend_idx ON friendships(friend_id);

-- 1:1 conversations; user_a < user_b always. last_seq is the per-conversation monotonic counter.
CREATE TABLE IF NOT EXISTS conversations (
    id         BIGSERIAL PRIMARY KEY,
    user_a     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    last_seq   BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    UNIQUE (user_a, user_b),
    CHECK (user_a < user_b)
);

-- Hash-partitioned by conv_id so hot conversations spread across partitions.
CREATE TABLE IF NOT EXISTS messages (
    conv_id    BIGINT   NOT NULL,
    seq        BIGINT   NOT NULL,
    sender     BIGINT   NOT NULL,
    kind       SMALLINT NOT NULL,
    body       BYTEA    NOT NULL,
    client_id  BIGINT   NOT NULL,
    created_at BIGINT   NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    PRIMARY KEY (conv_id, seq)
) PARTITION BY HASH (conv_id);

DO $$
BEGIN
    FOR i IN 0..7 LOOP
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS messages_p%s PARTITION OF messages FOR VALUES WITH (MODULUS 8, REMAINDER %s)', i, i);
    END LOOP;
END $$;

-- Idempotent sends: a retried (sender, client_id) returns the stored seq.
CREATE TABLE IF NOT EXISTS message_dedup (
    sender    BIGINT NOT NULL,
    client_id BIGINT NOT NULL,
    conv_id   BIGINT NOT NULL,
    seq       BIGINT NOT NULL,
    PRIMARY KEY (sender, client_id)
);

-- File metadata; chunk bytes live on disk under data/blobs/<hash>/<index>.
CREATE TABLE IF NOT EXISTS files (
    hash       BYTEA PRIMARY KEY,           -- blake3 of whole file
    size       BIGINT NOT NULL,
    chunks     INTEGER NOT NULL,
    complete   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint
);

-- Call history. id is the client-chosen call id (u64 reinterpreted as i64).
CREATE TABLE IF NOT EXISTS calls (
    id         BIGINT PRIMARY KEY,
    kind       TEXT   NOT NULL,             -- 'p2p' | 'room'
    started_by BIGINT NOT NULL REFERENCES users(id),
    started_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    ended_at   BIGINT
);
