// @generated automatically by Diesel CLI.

diesel::table! {
    assignments (id) {
        id -> Uuid,
        instructor_id -> Uuid,
        class_id -> Uuid,
        course_id -> Uuid,
    }
}

diesel::table! {
    classes (id) {
        id -> Uuid,
        year -> Int4,
        section -> Int4,
    }
}

diesel::table! {
    courses (id) {
        id -> Uuid,
        course_id -> Text,
        name -> Text,
    }
}

diesel::table! {
    enrollments (id) {
        id -> Uuid,
        student_id -> Uuid,
        course_id -> Uuid,
    }
}

diesel::table! {
    instructors (id) {
        id -> Uuid,
    }
}

diesel::table! {
    posts (id) {
        id -> Int4,
        title -> Varchar,
        body -> Text,
        published -> Bool,
    }
}

diesel::table! {
    profiles (id) {
        id -> Uuid,
        first_name -> Text,
        last_name -> Nullable<Text>,
        username -> Text,
        password_hash -> Text,
        img_url -> Nullable<Text>,
        role -> Text,
    }
}

diesel::table! {
    students (id) {
        id -> Uuid,
        class_id -> Uuid,
    }
}

diesel::joinable!(assignments -> classes (class_id));
diesel::joinable!(assignments -> courses (course_id));
diesel::joinable!(assignments -> instructors (instructor_id));
diesel::joinable!(enrollments -> courses (course_id));
diesel::joinable!(enrollments -> students (student_id));
diesel::joinable!(instructors -> profiles (id));
diesel::joinable!(students -> classes (class_id));
diesel::joinable!(students -> profiles (id));

diesel::allow_tables_to_appear_in_same_query!(
    assignments,
    classes,
    courses,
    enrollments,
    instructors,
    posts,
    profiles,
    students,
);
