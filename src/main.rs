mod models;
mod schema;

use crate::models::{Class, Enrollment, Student};
use axum::extract::Path;
use axum::http::StatusCode;
use axum::{
    Router,
    extract::{Json, Query, State},
    routing::{get, post},
};
use diesel::prelude::*;
use diesel::{BelongingToDsl, ExpressionMethods, HasQuery, QueryDsl};
use diesel_async::{
    AsyncConnection, AsyncPgConnection, RunQueryDsl,
    pooled_connection::{AsyncDieselConnectionManager, bb8},
};
use dotenvy::dotenv_override;
use models::{Assignment, Course, Instructor, Profile};
use serde::{Deserialize, Serialize};
use crate::schema::{assignments, classes, courses, enrollments, instructors, profiles, students};
use std::env;
use uuid::Uuid;

type Pool = bb8::Pool<AsyncPgConnection>;

pub async fn establish_connection() -> Result<AsyncPgConnection, Box<dyn std::error::Error>> {
    dotenv_override().ok();
    let database_url = env::var("DATABASE_URL").map_err(|_| format!("DATABASE_URL must be set"))?;
    println!("Database Url: {}", database_url);
    let conn = AsyncPgConnection::establish(&database_url)
        .await
        .map_err(|e| panic!("Error connecting to {}\n{}", database_url, e))?;
    Ok(conn)
}

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

#[derive(Deserialize)]
struct LoginData {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    user_id: String,
    role: String,
}

#[derive(Queryable, Serialize)]
struct StudentIds {
    id: Uuid,
}

#[derive(Deserialize)]
struct CourseClassID {
    course_id: String,
    class_id: String,
}

// ── New response types ────────────────────────────────────────────────────────

#[derive(Serialize, Queryable)]
struct StudentProfile {
    id: Uuid,
    nfc_id: String,
    first_name: String,
    last_name: Option<String>,
    username: String,
    img_url: Option<String>,
}

#[derive(Serialize)]
struct EnrichedAssignment {
    id: Uuid,
    instructor_id: Uuid,
    class_id: Uuid,
    course_id: Uuid,
    course_name: String,
    course_code: String,
    class_year: i32,
    class_section: i32,
    day: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    room: Option<String>,
}

#[derive(Serialize)]
struct ScheduleItem {
    course_id: Uuid,
    class_id: Uuid,
    course_name: String,
    class_year: String,
    class_section: String,
    day_of_week: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    room: Option<String>,
}

// ── Route handlers ────────────────────────────────────────────────────────────

async fn get_course_students_detailed(
    State(pool): State<Pool>,
    Query(ids): Query<CourseClassID>,
) -> Result<Json<Vec<StudentProfile>>, StatusCode> {
    let mut conn = pool.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let students = profiles::table
        .inner_join(students::table)
        .filter(students::class_id.eq(
            Uuid::parse_str(&ids.class_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ))
        .inner_join(enrollments::table.on(enrollments::student_id.eq(students::id)))
        .filter(enrollments::course_id.eq(
            Uuid::parse_str(&ids.course_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ))
        .select((
            profiles::id,
            students::nfc_id,
            profiles::first_name,
            profiles::last_name,
            profiles::username,
            profiles::img_url,
        ))
        .get_results::<StudentProfile>(&mut conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(students))
}

/// GET /instructor/assignment/enriched/:instructor_id
/// Returns assignments with joined course name and class details (B-08)
async fn get_instructor_assignment_enriched(
    State(pool): State<Pool>,
    Path(instructor_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<EnrichedAssignment>>), (StatusCode, Json<ErrorResponse>)> {
    let mut conn = pool.get().await.map_err(internal_error)?;

    let instructor_uuid = Uuid::parse_str(&instructor_id).map_err(internal_error)?;

    let profile = Profile::query()
        .filter(profiles::id.eq(instructor_uuid))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    let instructor = Instructor::query()
        .filter(instructors::id.eq(profile.id))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    // Load all assignments for instructor with course and class
    let raw: Vec<(Assignment, Course, Class)> = Assignment::belonging_to(&instructor)
        .inner_join(courses::table)
        .inner_join(classes::table)
        .select((Assignment::as_select(), Course::as_select(), Class::as_select()))
        .load(&mut conn)
        .await
        .map_err(bad_request_error)?;

    let enriched: Vec<EnrichedAssignment> = raw
        .into_iter()
        .map(|(a, c, cl)| EnrichedAssignment {
            id: a.id,
            instructor_id: a.instructor_id,
            class_id: a.class_id,
            course_id: a.course_id,
            course_name: c.name,
            course_code: c.course_id,
            class_year: cl.year,
            class_section: cl.section,
            day: a.day,
            start_time: a.start_time,
            end_time: a.end_time,
            room: a.room,
        })
        .collect();

    Ok((StatusCode::OK, Json(enriched)))
}

/// GET /schedule/:user_id  — returns timetable for a user (B-04)
async fn get_schedule(
    State(pool): State<Pool>,
    Path(user_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<ScheduleItem>>), (StatusCode, Json<ErrorResponse>)> {
    let mut conn = pool.get().await.map_err(internal_error)?;
    let user_uuid = Uuid::parse_str(&user_id).map_err(internal_error)?;

    // Determine role from profiles
    let profile = Profile::query()
        .filter(profiles::id.eq(user_uuid))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    let items: Vec<ScheduleItem> = if profile.role.to_lowercase() == "instructor" {
        // Instructor: load their assigned courses
        let instructor = Instructor::query()
            .filter(instructors::id.eq(user_uuid))
            .get_result(&mut conn)
            .await
            .map_err(bad_request_error)?;

        let raw: Vec<(Assignment, Course, Class)> = Assignment::belonging_to(&instructor)
            .inner_join(courses::table)
            .inner_join(classes::table)
            .select((Assignment::as_select(), Course::as_select(), Class::as_select()))
            .load(&mut conn)
            .await
            .map_err(bad_request_error)?;

        raw.into_iter()
            .map(|(a, c, cl)| ScheduleItem {
                course_id: a.course_id,
                class_id: a.class_id,
                course_name: c.name,
                class_year: cl.year.to_string(),
                class_section: cl.section.to_string(),
                day_of_week: a.day,
                start_time: a.start_time,
                end_time: a.end_time,
                room: a.room,
            })
            .collect()
    } else {
        // Student: load enrolled courses + their assignments
        let student = students::table
            .filter(students::id.eq(user_uuid))
            .get_result::<Student>(&mut conn)
            .await
            .map_err(bad_request_error)?;

        let enrolled_course_ids: Vec<Uuid> = Enrollment::belonging_to(&student)
            .select(enrollments::course_id)
            .load::<Uuid>(&mut conn)
            .await
            .map_err(bad_request_error)?;

        if enrolled_course_ids.is_empty() {
            return Ok((StatusCode::OK, Json(Vec::new())));
        }

        // Find assignments for those courses in the student's class
        let raw: Vec<(Assignment, Course, Class)> = assignments::table
            .filter(assignments::course_id.eq_any(&enrolled_course_ids))
            .filter(assignments::class_id.eq(student.class_id))
            .inner_join(courses::table)
            .inner_join(classes::table)
            .select((Assignment::as_select(), Course::as_select(), Class::as_select()))
            .load(&mut conn)
            .await
            .map_err(bad_request_error)?;

        raw.into_iter()
            .map(|(a, c, cl)| ScheduleItem {
                course_id: a.course_id,
                class_id: a.class_id,
                course_name: c.name,
                class_year: cl.year.to_string(),
                class_section: cl.section.to_string(),
                day_of_week: a.day,
                start_time: a.start_time,
                end_time: a.end_time,
                room: a.room,
            })
            .collect()
    };

    Ok((StatusCode::OK, Json(items)))
}

async fn get_class(
    State(pool): State<Pool>,
    Path(class_id): Path<String>,
) -> Result<(StatusCode, Json<Class>), (StatusCode, Json<ErrorResponse>)> {
    let mut conn = pool.get().await.map_err(internal_error)?;
    let class = Class::query()
        .filter(classes::id.eq(Uuid::parse_str(&class_id).map_err(internal_error)?))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    Ok((StatusCode::OK, Json(class)))
}

async fn get_course(
    State(pool): State<Pool>,
    Path(course_id): Path<String>,
) -> Result<(StatusCode, Json<Course>), (StatusCode, Json<ErrorResponse>)> {
    let mut conn = pool.get().await.map_err(internal_error)?;
    let course = Course::query()
        .filter(courses::id.eq(Uuid::parse_str(&course_id).map_err(internal_error)?))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    Ok((StatusCode::OK, Json(course)))
}

async fn get_instructor_assignment(
    State(pool): State<Pool>,
    Path(instructor_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<Assignment>>), (StatusCode, Json<ErrorResponse>)> {
    let mut conn = pool.get().await.map_err(internal_error)?;
    let profile = Profile::query()
        .filter(profiles::id.eq(Uuid::parse_str(&instructor_id).map_err(internal_error)?))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    let instructor = Instructor::query()
        .filter(instructors::id.eq(profile.id))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    let assignment_list = Assignment::belonging_to(&instructor)
        .select(Assignment::as_select())
        .load(&mut conn)
        .await
        .map_err(bad_request_error)?;

    Ok((StatusCode::OK, Json(assignment_list)))
}

async fn get_student_courses(
    State(pool): State<Pool>,
    Path(student_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<Course>>), (StatusCode, Json<ErrorResponse>)> {
    let mut conn = pool.get().await.map_err(internal_error)?;
    let profile = Profile::query()
        .filter(profiles::id.eq(Uuid::parse_str(&student_id).map_err(internal_error)?))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    let student = Student::query()
        .filter(students::id.eq(profile.id))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    let course_list = Enrollment::belonging_to(&student)
        .inner_join(courses::table)
        .select(Course::as_select())
        .load(&mut conn)
        .await
        .map_err(bad_request_error)?;

    Ok((StatusCode::OK, Json(course_list)))
}

#[derive(Deserialize, Debug)]
struct Ids {
    id: Option<Uuid>,
    nfc_id: Option<String>,
}

async fn get_student_profile(
    State(pool): State<Pool>,
    Query(ids): Query<Ids>,
) -> Result<Json<StudentProfile>, (StatusCode, Json<ErrorResponse>)> {
    println!("{:?}", ids);
    if ids.id.is_none() && ids.nfc_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "student id missing.".to_string(),
            }),
        ));
    }

    let mut conn = pool.get().await.map_err(internal_error)?;
    let student = if ids.id.is_some() {
        let student_id = ids.id.unwrap();
        students::table
            .inner_join(profiles::table)
            .filter(students::id.eq(student_id))
            .select((
                profiles::id,
                students::nfc_id,
                profiles::first_name,
                profiles::last_name,
                profiles::username,
                profiles::img_url,
            ))
            .get_result::<StudentProfile>(&mut conn)
            .await
            .map_err(|_| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        message: "student not found".to_string(),
                    }),
                )
            })?
    } else {
        let nfc_id = ids.nfc_id.unwrap();
        students::table
            .inner_join(profiles::table)
            .filter(students::nfc_id.eq(nfc_id))
            .select((
                profiles::id,
                students::nfc_id,
                profiles::first_name,
                profiles::last_name,
                profiles::username,
                profiles::img_url,
            ))
            .get_result::<StudentProfile>(&mut conn)
            .await
            .map_err(|_| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        message: "student not found".to_string(),
                    }),
                )
            })?
    };

    Ok(Json(student))
}

#[derive(Serialize)]
struct UserProfile {
    id: String,
    username: String,
    first_name: String,
    last_name: Option<String>,
    role: String,
    img_url: Option<String>,
}

async fn get_user_profile(
    State(pool): State<Pool>,
    Path(user_id): Path<String>,
) -> Result<(StatusCode, Json<UserProfile>), (StatusCode, Json<ErrorResponse>)> {
    let mut conn = pool.get().await.map_err(internal_error)?;
    let profile = Profile::query()
        .filter(profiles::id.eq(Uuid::parse_str(&user_id).map_err(internal_error)?))
        .get_result(&mut conn)
        .await
        .map_err(bad_request_error)?;

    Ok((
        StatusCode::OK,
        Json(UserProfile {
            id: profile.id.to_string(),
            username: profile.username,
            first_name: profile.first_name,
            last_name: profile.last_name,
            role: profile.role,
            img_url: profile.img_url,
        }),
    ))
}

async fn login_handler(
    State(pool): State<Pool>,
    Json(payload): Json<LoginData>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let profile = Profile::query()
        .filter(profiles::username.eq(payload.username))
        .get_result(&mut conn)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    if profile.password_hash != payload.password {
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(Json(LoginResponse {
        user_id: profile.id.to_string(),
        role: profile.role,
    }))
}

#[tokio::main]
async fn main() {
    dotenv_override().ok();
    let db_url = std::env::var("DATABASE_URL").unwrap();

    println!("Database Url: {}", db_url);
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(db_url);
    let pool = bb8::Pool::builder().build(config).await.unwrap();

    println!("Database Connection Established!");
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/login", post(login_handler))
        .route("/user/{user_id}", get(get_user_profile))
        .route("/student/courses/{student_id}", get(get_student_courses))
        .route("/course/{course_id}", get(get_course))
        .route("/student", get(get_course_students_detailed))
        .route("/student/profile", get(get_student_profile))
        .route("/class/{class_id}", get(get_class))
        .route(
            "/instructor/assignment/{instructor_id}",
            get(get_instructor_assignment),
        )
        // B-08: enriched assignments (course_name + class info joined)
        .route(
            "/instructor/assignment/enriched/{instructor_id}",
            get(get_instructor_assignment_enriched),
        )
        // B-04: schedule/timetable
        .route("/schedule/{user_id}", get(get_schedule))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on port 3000");
    axum::serve(listener, app).await.unwrap();
}

fn internal_error<E>(err: E) -> (StatusCode, Json<ErrorResponse>)
where
    E: std::error::Error,
{
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            message: err.to_string(),
        }),
    )
}

fn bad_request_error<E>(err: E) -> (StatusCode, Json<ErrorResponse>)
where
    E: std::error::Error,
{
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            message: err.to_string(),
        }),
    )
}
