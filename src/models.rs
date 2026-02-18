use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

#[derive(HasQuery, Insertable, Debug)]
#[diesel(table_name = crate::schema::profiles)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Profile {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: String,
    pub password_hash: String,
    pub img_url: Option<String>,
    pub role: String,
}

#[derive(HasQuery, Insertable, Serialize, Debug)]
#[diesel(table_name = crate::schema::classes)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Class {
    pub id: Uuid,
    pub year: i32,
    pub section: i32,
}

#[derive(HasQuery, Insertable, Identifiable, Debug)]
#[diesel(table_name = crate::schema::students)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Student {
    pub id: Uuid,
    pub class_id: Uuid,
    pub nfc_id: String,
}

#[derive(Identifiable, Queryable, Selectable, Insertable, Associations, Debug)]
#[diesel(table_name = crate::schema::enrollments)]
#[diesel(belongs_to(Student))]
#[diesel(belongs_to(Course))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Enrollment {
    pub id: Uuid,
    pub student_id: Uuid,
    pub course_id: Uuid,
}

#[derive(Identifiable, HasQuery, Associations, Insertable, Serialize, Debug)]
#[diesel(table_name = crate::schema::assignments)]
#[diesel(belongs_to(Instructor))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Assignment {
    pub id: Uuid,
    pub instructor_id: Uuid,
    pub class_id: Uuid,
    pub course_id: Uuid,
}

#[derive(Identifiable, HasQuery, Insertable, Serialize, Debug)]
#[diesel(table_name = crate::schema::courses)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Course {
    pub id: Uuid,
    pub course_id: String,
    pub name: String,
}

#[derive(HasQuery, Insertable, Identifiable, Debug)]
#[diesel(table_name = crate::schema::instructors)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Instructor {
    pub id: Uuid,
}
