-- Your SQL goes here
CREATE TABLE instructors (
    id UUID PRIMARY KEY,

    FOREIGN KEY(id) REFERENCES users(id) ON DELETE CASCADE
)
