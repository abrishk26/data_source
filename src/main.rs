mod models;
mod schema;

use diesel::prelude::*;
use dotenvy::dotenv;
use models::{Class, Profile, Student};
use std::env;
use uuid::Uuid;

use crate::schema::{classes, profiles, students};

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

fn main() {
    let mut conn = establish_connection();
    let user_id = Uuid::now_v7();
    let class_id = Uuid::now_v7();

    let profile = Profile {
        id: user_id,
        first_name: "Abreham".to_string(),
        last_name: None,
        username: "UGR/0209/15".to_string(),
        password_hash: "strong_password_hash".to_string(),
        img_url: None,
        role: "student".to_string(),
    };

    diesel::insert_into(profiles::table)
        .values(&profile)
        .execute(&mut conn)
        .expect("Error saving new user");

    let class = Class {
        id: class_id,
        year: 4,
        section: 2,
    };

    diesel::insert_into(classes::table)
        .values(&class)
        .execute(&mut conn)
        .expect("Error saving new class");

    let student = Student {
        id: user_id,
        class_id: class_id,
    };

    diesel::insert_into(students::table)
        .values(&student)
        .execute(&mut conn)
        .expect("Error saving new student");

    let student_with_profile = students::table
        .inner_join(profiles::table)
        .filter(profiles::id.eq(user_id))
        .select((Student::as_select(), Profile::as_select()))
        .load::<(Student, Profile)>(&mut conn)
        .expect("failed to retrieve data");

    println!("Profile: {:?}", student_with_profile[0].0);
    println!("User: {:?}", student_with_profile[0].1);
}
