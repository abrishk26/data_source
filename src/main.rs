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
// use models::{Class, Profile, Student};
use crate::schema::{classes, courses, enrollments, instructors, profiles, students};
use std::env;
use uuid::Uuid;

type Pool = bb8::Pool<AsyncPgConnection>;

// use crate::schema::{classes, profiles, students};

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

async fn get_course_students_detailed(
    State(pool): State<Pool>,
    Query(ids): Query<CourseClassID>,
) -> Result<Json<Vec<StudentProfile>>, StatusCode> {
    let mut conn = pool
        .get()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

#[tokio::main]
async fn main() {
    dotenv_override().ok();
    let db_url = std::env::var("DATABASE_URL").unwrap();

    println!("Database Url: {}", db_url);
    // set up connection pool
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
        .with_state(pool);

    let listenter = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on port 3000");
    axum::serve(listenter, app).await.unwrap();
}

#[derive(Serialize)]
struct UserProfile {
    id: String,
    username: String,
    first_name: String,
    last_name: Option<String>,
    role: String,
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

    let assignments = Assignment::belonging_to(&instructor)
        .select(Assignment::as_select())
        .load(&mut conn)
        .await
        .map_err(bad_request_error)?;

    Ok((StatusCode::OK, Json(assignments)))
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

    let courses = Enrollment::belonging_to(&student)
        .inner_join(courses::table)
        .select(Course::as_select())
        .load(&mut conn)
        .await
        .map_err(bad_request_error)?;

    Ok((StatusCode::OK, Json(courses)))
}

#[derive(Deserialize, Debug)]
struct Ids {
    id: Option<Uuid>,
    nfc_id: Option<String>,
}

#[derive(Serialize, Queryable)]
struct StudentProfile {
    id: Uuid,
    nfc_id: String,
    first_name: String,
    last_name: Option<String>,
    username: String,
    img_url: Option<String>,
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
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        message: "internal server error".to_string(),
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
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        message: "internal server error".to_string(),
                    }),
                )
            })?
    };

    Ok(Json(student))
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

    return Ok(Json(LoginResponse {
        user_id: profile.id.to_string(),
        role: profile.role,
    }));
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
