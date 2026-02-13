-- Your SQL goes here
CREATE TABLE profiles (
    id UUID PRIMARY KEY,
    first_name TEXT NOT NULL,
    last_name TEXT,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    img_url TEXT,
    role TEXT NOT NULL
)

