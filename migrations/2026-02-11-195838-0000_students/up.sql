-- Your SQL goes here
CREATE TABLE students (
    id UUID PRIMARY KEY,
    class_id UUID NOT NULL,

    FOREIGN KEY(id) REFERENCES profiles(id) ON DELETE CASCADE,
    FOREIGN KEY(class_id) REFERENCES classes(id) ON UPDATE CASCADE
)
