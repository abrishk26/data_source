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
use tower_http::trace::{TraceLayer, DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse};
use tracing::{debug, error, info, warn, Level};
use uuid::Uuid;

type Pool = bb8::Pool<AsyncPgConnection>;

pub async fn establish_connection() -> Result<AsyncPgConnection, Box<dyn std::error::Error>> {
    dotenv_override().ok();
    let database_url = env::var("DATABASE_URL").map_err(|_| format!("DATABASE_URL must be set"))?;
    info!(url = %database_url, "Connecting to database");
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
    role: String,
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
    info!(course_id = %ids.course_id, class_id = %ids.class_id, "GET /student — fetching detailed student list");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let class_uuid = Uuid::parse_str(&ids.class_id).map_err(|e| {
        warn!(class_id = %ids.class_id, error = %e, "Invalid class_id UUID");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let course_uuid = Uuid::parse_str(&ids.course_id).map_err(|e| {
        warn!(course_id = %ids.course_id, error = %e, "Invalid course_id UUID");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    debug!(class_id = %class_uuid, course_id = %course_uuid, "Querying students for class/course");

    let students = profiles::table
        .inner_join(students::table)
        .filter(students::class_id.eq(class_uuid))
        .inner_join(enrollments::table.on(enrollments::student_id.eq(students::id)))
        .filter(enrollments::course_id.eq(course_uuid))
        .select((
            profiles::id,
            students::nfc_id,
            profiles::first_name,
            profiles::last_name,
            profiles::username,
            profiles::img_url,
            profiles::role,
        ))
        .get_results::<StudentProfile>(&mut conn)
        .await
        .map_err(|e| {
            error!(error = %e, "DB query failed for get_course_students_detailed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(count = students.len(), "Returning student profiles");
    Ok(Json(students))
}

/// GET /instructor/assignment/enriched/:instructor_id
/// Returns assignments with joined course name and class details (B-08)
async fn get_instructor_assignment_enriched(
    State(pool): State<Pool>,
    Path(instructor_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<EnrichedAssignment>>), (StatusCode, Json<ErrorResponse>)> {
    info!(instructor_id = %instructor_id, "GET /instructor/assignment/enriched — fetching enriched assignments");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let instructor_uuid = Uuid::parse_str(&instructor_id).map_err(|e| {
        warn!(instructor_id = %instructor_id, error = %e, "Invalid instructor_id UUID");
        internal_error(e)
    })?;

    debug!(instructor_id = %instructor_uuid, "Looking up instructor profile");
    let profile = Profile::query()
        .filter(profiles::id.eq(instructor_uuid))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(instructor_id = %instructor_uuid, error = %e, "Profile not found for instructor");
            bad_request_error(e)
        })?;

    debug!(profile_id = %profile.id, "Looking up instructor record");
    let instructor = Instructor::query()
        .filter(instructors::id.eq(profile.id))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(profile_id = %profile.id, error = %e, "Instructor record not found");
            bad_request_error(e)
        })?;

    debug!(instructor_id = %instructor_uuid, "Loading assignments with course and class joins");
    let raw: Vec<(Assignment, Course, Class)> = Assignment::belonging_to(&instructor)
        .inner_join(courses::table)
        .inner_join(classes::table)
        .select((Assignment::as_select(), Course::as_select(), Class::as_select()))
        .load(&mut conn)
        .await
        .map_err(|e| {
            error!(instructor_id = %instructor_uuid, error = %e, "DB query failed loading enriched assignments");
            bad_request_error(e)
        })?;

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

    info!(instructor_id = %instructor_uuid, count = enriched.len(), "Returning enriched assignments");
    Ok((StatusCode::OK, Json(enriched)))
}

/// GET /schedule/:user_id  — returns timetable for a user (B-04)
async fn get_schedule(
    State(pool): State<Pool>,
    Path(user_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<ScheduleItem>>), (StatusCode, Json<ErrorResponse>)> {
    info!(user_id = %user_id, "GET /schedule — fetching schedule");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let user_uuid = Uuid::parse_str(&user_id).map_err(|e| {
        warn!(user_id = %user_id, error = %e, "Invalid user_id UUID");
        internal_error(e)
    })?;

    debug!(user_id = %user_uuid, "Looking up user profile to determine role");
    let profile = Profile::query()
        .filter(profiles::id.eq(user_uuid))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(user_id = %user_uuid, error = %e, "Profile not found");
            bad_request_error(e)
        })?;

    info!(user_id = %user_uuid, role = %profile.role, "Resolved user role for schedule");

    let items: Vec<ScheduleItem> = if profile.role.to_lowercase() == "instructor" {
        debug!(user_id = %user_uuid, "Loading instructor schedule");

        let instructor = Instructor::query()
            .filter(instructors::id.eq(user_uuid))
            .get_result(&mut conn)
            .await
            .map_err(|e| {
                warn!(user_id = %user_uuid, error = %e, "Instructor record not found");
                bad_request_error(e)
            })?;

        let raw: Vec<(Assignment, Course, Class)> = Assignment::belonging_to(&instructor)
            .inner_join(courses::table)
            .inner_join(classes::table)
            .select((Assignment::as_select(), Course::as_select(), Class::as_select()))
            .load(&mut conn)
            .await
            .map_err(|e| {
                error!(user_id = %user_uuid, error = %e, "DB query failed loading instructor schedule");
                bad_request_error(e)
            })?;

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
        debug!(user_id = %user_uuid, "Loading student schedule");

        let student = students::table
            .filter(students::id.eq(user_uuid))
            .get_result::<Student>(&mut conn)
            .await
            .map_err(|e| {
                warn!(user_id = %user_uuid, error = %e, "Student record not found");
                bad_request_error(e)
            })?;

        let enrolled_course_ids: Vec<Uuid> = Enrollment::belonging_to(&student)
            .select(enrollments::course_id)
            .load::<Uuid>(&mut conn)
            .await
            .map_err(|e| {
                error!(user_id = %user_uuid, error = %e, "DB query failed loading enrollments");
                bad_request_error(e)
            })?;

        if enrolled_course_ids.is_empty() {
            warn!(user_id = %user_uuid, "Student has no enrollments — returning empty schedule");
            return Ok((StatusCode::OK, Json(Vec::new())));
        }

        debug!(user_id = %user_uuid, course_count = enrolled_course_ids.len(), "Loading assignments for enrolled courses");

        let raw: Vec<(Assignment, Course, Class)> = assignments::table
            .filter(assignments::course_id.eq_any(&enrolled_course_ids))
            .filter(assignments::class_id.eq(student.class_id))
            .inner_join(courses::table)
            .inner_join(classes::table)
            .select((Assignment::as_select(), Course::as_select(), Class::as_select()))
            .load(&mut conn)
            .await
            .map_err(|e| {
                error!(user_id = %user_uuid, error = %e, "DB query failed loading student schedule");
                bad_request_error(e)
            })?;

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

    info!(user_id = %user_uuid, count = items.len(), "Returning schedule items");
    Ok((StatusCode::OK, Json(items)))
}

async fn get_class(
    State(pool): State<Pool>,
    Path(class_id): Path<String>,
) -> Result<(StatusCode, Json<Class>), (StatusCode, Json<ErrorResponse>)> {
    info!(class_id = %class_id, "GET /class — fetching class");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let class_uuid = Uuid::parse_str(&class_id).map_err(|e| {
        warn!(class_id = %class_id, error = %e, "Invalid class_id UUID");
        internal_error(e)
    })?;

    debug!(class_id = %class_uuid, "Querying class record");
    let class = Class::query()
        .filter(classes::id.eq(class_uuid))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(class_id = %class_uuid, error = %e, "Class not found");
            bad_request_error(e)
        })?;

    info!(class_id = %class_uuid, "Returning class");
    Ok((StatusCode::OK, Json(class)))
}

async fn get_course(
    State(pool): State<Pool>,
    Path(course_id): Path<String>,
) -> Result<(StatusCode, Json<Course>), (StatusCode, Json<ErrorResponse>)> {
    info!(course_id = %course_id, "GET /course — fetching course");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let course_uuid = Uuid::parse_str(&course_id).map_err(|e| {
        warn!(course_id = %course_id, error = %e, "Invalid course_id UUID");
        internal_error(e)
    })?;

    debug!(course_id = %course_uuid, "Querying course record");
    let course = Course::query()
        .filter(courses::id.eq(course_uuid))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(course_id = %course_uuid, error = %e, "Course not found");
            bad_request_error(e)
        })?;

    info!(course_id = %course_uuid, "Returning course");
    Ok((StatusCode::OK, Json(course)))
}

async fn get_instructor_assignment(
    State(pool): State<Pool>,
    Path(instructor_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<Assignment>>), (StatusCode, Json<ErrorResponse>)> {
    info!(instructor_id = %instructor_id, "GET /instructor/assignment — fetching assignments");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let instructor_uuid = Uuid::parse_str(&instructor_id).map_err(|e| {
        warn!(instructor_id = %instructor_id, error = %e, "Invalid instructor_id UUID");
        internal_error(e)
    })?;

    debug!(instructor_id = %instructor_uuid, "Looking up instructor profile");
    let profile = Profile::query()
        .filter(profiles::id.eq(instructor_uuid))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(instructor_id = %instructor_uuid, error = %e, "Profile not found for instructor");
            bad_request_error(e)
        })?;

    debug!(profile_id = %profile.id, "Looking up instructor record");
    let instructor = Instructor::query()
        .filter(instructors::id.eq(profile.id))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(profile_id = %profile.id, error = %e, "Instructor record not found");
            bad_request_error(e)
        })?;

    debug!(instructor_id = %instructor_uuid, "Loading assignment list");
    let assignment_list = Assignment::belonging_to(&instructor)
        .select(Assignment::as_select())
        .load(&mut conn)
        .await
        .map_err(|e| {
            error!(instructor_id = %instructor_uuid, error = %e, "DB query failed loading assignments");
            bad_request_error(e)
        })?;

    info!(instructor_id = %instructor_uuid, count = assignment_list.len(), "Returning assignments");
    Ok((StatusCode::OK, Json(assignment_list)))
}

async fn get_student_courses(
    State(pool): State<Pool>,
    Path(student_id): Path<String>,
) -> Result<(StatusCode, Json<Vec<Course>>), (StatusCode, Json<ErrorResponse>)> {
    info!(student_id = %student_id, "GET /student/courses — fetching student courses");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let student_uuid = Uuid::parse_str(&student_id).map_err(|e| {
        warn!(student_id = %student_id, error = %e, "Invalid student_id UUID");
        internal_error(e)
    })?;

    debug!(student_id = %student_uuid, "Looking up student profile");
    let profile = Profile::query()
        .filter(profiles::id.eq(student_uuid))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(student_id = %student_uuid, error = %e, "Profile not found for student");
            bad_request_error(e)
        })?;

    debug!(profile_id = %profile.id, "Looking up student record");
    let student = Student::query()
        .filter(students::id.eq(profile.id))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(profile_id = %profile.id, error = %e, "Student record not found");
            bad_request_error(e)
        })?;

    debug!(student_id = %student_uuid, "Loading enrolled courses");
    let course_list = Enrollment::belonging_to(&student)
        .inner_join(courses::table)
        .select(Course::as_select())
        .load(&mut conn)
        .await
        .map_err(|e| {
            error!(student_id = %student_uuid, error = %e, "DB query failed loading student courses");
            bad_request_error(e)
        })?;

    info!(student_id = %student_uuid, count = course_list.len(), "Returning student courses");
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
    debug!(query = ?ids, "GET /student/profile — received query params");

    if ids.id.is_none() && ids.nfc_id.is_none() {
        warn!("GET /student/profile called with no id or nfc_id");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "student id missing.".to_string(),
            }),
        ));
    }

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let student = if let Some(student_id) = ids.id {
        info!(student_id = %student_id, "Looking up student profile by id");
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
                profiles::role,
            ))
            .get_result::<StudentProfile>(&mut conn)
            .await
            .map_err(|e| {
                warn!(student_id = %student_id, error = %e, "Student not found by id");
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        message: "student not found".to_string(),
                    }),
                )
            })?
    } else {
        let nfc_id = ids.nfc_id.unwrap();
        info!(nfc_id = %nfc_id, "Looking up student profile by nfc_id");
        students::table
            .inner_join(profiles::table)
            .filter(students::nfc_id.eq(&nfc_id))
            .select((
                profiles::id,
                students::nfc_id,
                profiles::first_name,
                profiles::last_name,
                profiles::username,
                profiles::img_url,
                profiles::role,
            ))
            .get_result::<StudentProfile>(&mut conn)
            .await
            .map_err(|e| {
                warn!(nfc_id = %nfc_id, error = %e, "Student not found by nfc_id");
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        message: "student not found".to_string(),
                    }),
                )
            })?
    };

    info!(student_id = %student.id, "Returning student profile");
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
    info!(user_id = %user_id, "GET /user — fetching user profile");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        internal_error(e)
    })?;

    let user_uuid = Uuid::parse_str(&user_id).map_err(|e| {
        warn!(user_id = %user_id, error = %e, "Invalid user_id UUID");
        internal_error(e)
    })?;

    debug!(user_id = %user_uuid, "Querying user profile");
    let profile = Profile::query()
        .filter(profiles::id.eq(user_uuid))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(user_id = %user_uuid, error = %e, "User profile not found");
            bad_request_error(e)
        })?;

    info!(user_id = %user_uuid, role = %profile.role, "Returning user profile");
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
    info!(username = %payload.username, "POST /login — login attempt");

    let mut conn = pool.get().await.map_err(|e| {
        error!(error = %e, "Failed to acquire DB connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    debug!(username = %payload.username, "Looking up profile by username");
    let profile = Profile::query()
        .filter(profiles::username.eq(&payload.username))
        .get_result(&mut conn)
        .await
        .map_err(|e| {
            warn!(username = %payload.username, error = %e, "Login failed — user not found");
            StatusCode::BAD_REQUEST
        })?;

    if profile.password_hash != payload.password {
        warn!(username = %payload.username, "Login failed — incorrect password");
        return Err(StatusCode::BAD_REQUEST);
    }

    info!(user_id = %profile.id, role = %profile.role, "Login successful");
    Ok(Json(LoginResponse {
        user_id: profile.id.to_string(),
        role: profile.role,
    }))
}

#[tokio::main]
async fn main() {
    // Initialise tracing — respects RUST_LOG env var, defaults to "info"
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    dotenv_override().ok();
    let db_url = std::env::var("DATABASE_URL").unwrap();

    info!(url = %db_url, "Connecting to database");
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(db_url);
    let pool = bb8::Pool::builder().build(config).await.unwrap();

    info!("Database connection pool established");
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
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(pool);

    let server_port = std::env::var("SERVER_PORT")
        .unwrap_or_else(|_| "0".to_string());
    let bind_addr = format!("0.0.0.0:{}", server_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    info!(address = %addr, "Server listening");
    axum::serve(listener, app).await.unwrap();
}

fn internal_error<E>(err: E) -> (StatusCode, Json<ErrorResponse>)
where
    E: std::error::Error,
{
    error!(error = %err, "Internal server error");
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
    warn!(error = %err, "Bad request error");
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            message: err.to_string(),
        }),
    )
}
