-- Your SQL goes here
CREATE TABLE assignments (
    id UUID PRIMARY KEY,
    instructor_id UUID NOT NULL ,
    class_id UUID NOT NULL,
    course_id UUID NOT NULL,

    FOREIGN KEY(instructor_id) REFERENCES instructors(id),
    FOREIGN KEY(class_id) REFERENCES classes(id),
    FOREIGN KEY(course_id) REFERENCES courses(id)
)
