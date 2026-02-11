-- Your SQL goes here
CREATE TABLE assignments (
    id UUID PRIMARY KEY,
    instructor_id UUID,
    class_id UUID,
    course_id UUID,

    FOREIGN KEY(instructor_id) REFERENCES instructors(id),
    FOREIGN KEY(class_id) REFERENCES classes(id),
    FOREIGN KEY(course_id) REFERENCES courses(id)
)
