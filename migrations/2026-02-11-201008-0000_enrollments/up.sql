-- Your SQL goes here
CREATE TABLE enrollments (
    id UUID PRIMARY KEY,
    student_id UUID,
    course_id UUID,

    FOREIGN KEY(student_id) REFERENCES students(id) ON DELETE CASCADE,
    FOREIGN KEY(course_id) REFERENCES courses(id) ON DELETE CASCADE
)
