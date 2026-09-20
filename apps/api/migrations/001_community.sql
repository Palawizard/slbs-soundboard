CREATE TABLE users (
    id uuid PRIMARY KEY,
    google_subject varchar(255) NOT NULL UNIQUE,
    email varchar(320) NOT NULL,
    username varchar(32) NOT NULL,
    username_key varchar(32) NOT NULL UNIQUE,
    avatar_url text,
    disabled_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (username ~ '^[A-Za-z0-9_]{3,32}$'),
    CHECK (username_key = lower(username))
);

CREATE TABLE oauth_transactions (
    state_hash bytea PRIMARY KEY,
    redirect_uri text NOT NULL,
    code_challenge varchar(128) NOT NULL,
    nonce varchar(128) NOT NULL,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    last_used_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE media_assets (
    id uuid PRIMARY KEY,
    kind varchar(8) NOT NULL CHECK (kind IN ('audio', 'image')),
    sha256 char(64) NOT NULL,
    mime_type varchar(100) NOT NULL,
    extension varchar(12) NOT NULL,
    byte_size bigint NOT NULL CHECK (byte_size > 0),
    duration_ms integer CHECK (duration_ms > 0),
    width integer CHECK (width > 0),
    height integer CHECK (height > 0),
    storage_key text NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (kind, sha256)
);

CREATE TABLE user_media (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    asset_id uuid NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    deleted_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, asset_id)
);

CREATE TABLE publications (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    audio_media_id uuid NOT NULL REFERENCES user_media(id) ON DELETE RESTRICT,
    image_media_id uuid REFERENCES user_media(id) ON DELETE RESTRICT,
    title varchar(120) NOT NULL,
    description varchar(500) NOT NULL DEFAULT '',
    status varchar(12) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled', 'deleted')),
    disabled_reason varchar(240),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz,
    CHECK (length(trim(title)) BETWEEN 1 AND 120)
);

CREATE TABLE audit_events (
    id bigserial PRIMARY KEY,
    actor_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    action varchar(80) NOT NULL,
    target_type varchar(40) NOT NULL,
    target_id text,
    request_id varchar(128),
    source_ip inet,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX sessions_active_idx ON sessions(token_hash, expires_at) WHERE revoked_at IS NULL;
CREATE INDEX oauth_transactions_expiry_idx ON oauth_transactions(expires_at);
CREATE INDEX publications_feed_idx ON publications(created_at DESC, id DESC) WHERE status = 'active';
CREATE INDEX publications_owner_idx ON publications(owner_id, created_at DESC);
CREATE INDEX publications_search_idx ON publications USING gin (to_tsvector('simple', title || ' ' || description)) WHERE status = 'active';
CREATE INDEX user_media_owner_idx ON user_media(user_id, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX audit_events_created_idx ON audit_events(created_at DESC);
