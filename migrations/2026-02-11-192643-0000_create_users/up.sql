-- Your SQL goes here
CREATE TABLE users (
    id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    first_name TEXT NOT NULL,
    last_name TEXT,
    password_hash TEXT NOT NULL,
    img_url TEXT
)

