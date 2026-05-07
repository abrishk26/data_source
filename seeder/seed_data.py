import os
import uuid
import random
import psycopg2
from faker import Faker
from dotenv import load_dotenv

# Load environment variables from .env if it exists
load_dotenv()

fake = Faker()

# Default to the URL mentioned in the README if not provided in environment
DATABASE_URL = os.environ.get("DATABASE_URL", "postgresql://postgres:postgres@localhost:5432/as")

def get_connection():
    """Establish connection to the PostgreSQL database."""
    try:
        conn = psycopg2.connect(DATABASE_URL)
        return conn
    except Exception as e:
        print(f"Error connecting to database: {e}")
        print(f"Used DATABASE_URL: {DATABASE_URL}")
        return None

def seed_data():
    """Seed the database with mock data for students, instructors, and courses."""
    conn = get_connection()
    if not conn:
        return

    cur = conn.cursor()

    try:
        print("--- Starting Database Seeding ---")

        # 1. Clear existing data (Optional, but ensures a clean state)
        # Uncomment the following lines if you want to start fresh:
        # print("Clearing existing data...")
        # cur.execute("TRUNCATE assignments, enrollments, students, instructors, courses, classes, profiles CASCADE")

        # 2. Seed Classes
        print("Seeding classes...")
        class_ids = []
        for year in range(1, 5):  # 4 years
            for section in range(1, 3):  # 2 sections per year
                cid = str(uuid.uuid4())
                cur.execute(
                    "INSERT INTO classes (id, year, section) VALUES (%s, %s, %s)",
                    (cid, year, section)
                )
                class_ids.append(cid)

        # 3. Seed Profiles (Admin, Instructors, Students)
        print("Seeding profiles...")
        
        # Admin Account
        admin_id = str(uuid.uuid4())
        cur.execute(
            "INSERT INTO profiles (id, first_name, last_name, username, password_hash, role) VALUES (%s, %s, %s, %s, %s, %s)",
            (admin_id, "System", "Admin", "admin", "admin123", "admin")
        )

        # Instructor Accounts
        instructor_ids = []
        # Predefined instructors for consistency with README
        predefined_instructors = [
            ("Mekaeel", "Mekaeel", "dr.mekaeel")
        ]
        
        for first, last, uname in predefined_instructors:
            iid = str(uuid.uuid4())
            cur.execute(
                "INSERT INTO profiles (id, first_name, last_name, username, password_hash, role) VALUES (%s, %s, %s, %s, %s, %s)",
                (iid, first, last, uname, "admin123", "instructor")
            )
            cur.execute("INSERT INTO instructors (id) VALUES (%s)", (iid,))
            instructor_ids.append(iid)

        # Generate 4 more random instructors
        for _ in range(4):
            ifirst = fake.first_name()
            ilast = fake.last_name()
            iuname = f"prof.{ifirst.lower()}"
            iid = str(uuid.uuid4())
            cur.execute(
                "INSERT INTO profiles (id, first_name, last_name, username, password_hash, role) VALUES (%s, %s, %s, %s, %s, %s)",
                (iid, ifirst, ilast, iuname, "admin123", "instructor")
            )
            cur.execute("INSERT INTO instructors (id) VALUES (%s)", (iid,))
            instructor_ids.append(iid)

        # Student Accounts
        student_data = [] # List of tuples (id, class_id)
        # Predefined students from README
        predefined_students = [
            ("Alice", "Johnson", "alice.j"),
            ("Bob", "Smith", "bob.s"),
            ("Charlie", "Brown", "charlie.b"),
            ("Diana", "Prince", "diana.p")
        ]

        for first, last, uname in predefined_students:
            sid = str(uuid.uuid4())
            cur.execute(
                "INSERT INTO profiles (id, first_name, last_name, username, password_hash, role) VALUES (%s, %s, %s, %s, %s, %s)",
                (sid, first, last, uname, "password123", "student")
            )
            cid = random.choice(class_ids)
            nfc = str(uuid.uuid4())[:8].upper()
            cur.execute(
                "INSERT INTO students (id, class_id, nfc_id) VALUES (%s, %s, %s)",
                (sid, cid, nfc)
            )
            student_data.append((sid, cid))

        # Generate 46 more random students
        for _ in range(46):
            sfirst = fake.first_name()
            slast = fake.last_name()
            suname = f"{sfirst.lower()}.{random.randint(100, 999)}"
            sid = str(uuid.uuid4())
            cur.execute(
                "INSERT INTO profiles (id, first_name, last_name, username, password_hash, role) VALUES (%s, %s, %s, %s, %s, %s)",
                (sid, sfirst, slast, suname, "password123", "student")
            )
            cid = random.choice(class_ids)
            nfc = str(uuid.uuid4())[:8].upper()
            cur.execute(
                "INSERT INTO students (id, class_id, nfc_id) VALUES (%s, %s, %s)",
                (sid, cid, nfc)
            )
            student_data.append((sid, cid))

        # 4. Seed Courses
        print("Seeding courses...")
        course_list = [
            ("CS101", "Introduction to Programming"),
            ("CS102", "Data Structures & Algorithms"),
            ("CS201", "Database Management Systems"),
            ("CS202", "Software Engineering"),
            ("CS301", "Artificial Intelligence"),
            ("CS302", "Computer Networks"),
            ("CS401", "Operating Systems"),
            ("CS402", "Cloud Computing"),
            ("MATH101", "Calculus I"),
            ("MATH201", "Linear Algebra"),
            ("PHYS101", "Engineering Physics"),
            ("HUM101", "Professional Ethics"),
            ("ENG101", "Academic Writing"),
            ("MGMT201", "Project Management"),
            ("SURE101", "Sustainable Energy")
        ]
        course_ids = []
        for code, name in course_list:
            cuid = str(uuid.uuid4())
            cur.execute(
                "INSERT INTO courses (id, course_id, name) VALUES (%s, %s, %s)",
                (cuid, code, name)
            )
            course_ids.append(cuid)

        # 5. Seed Enrollments
        print("Seeding enrollments...")
        for sid, _ in student_data:
            # Each student enrolled in 4-6 random courses
            selected_courses = random.sample(course_ids, random.randint(4, 6))
            for cuid in selected_courses:
                cur.execute(
                    "INSERT INTO enrollments (id, student_id, course_id) VALUES (%s, %s, %s)",
                    (str(uuid.uuid4()), sid, cuid)
                )

        # 6. Seed Assignments
        print("Seeding instructor assignments...")
        for iid in instructor_ids:
            # Each instructor assigned to 3-5 random class-course combinations
            for _ in range(random.randint(3, 5)):
                cid = random.choice(class_ids)
                cuid = random.choice(course_ids)
                # Ensure no duplicate assignments (simple check)
                cur.execute(
                    "INSERT INTO assignments (id, instructor_id, class_id, course_id) VALUES (%s, %s, %s, %s)",
                    (str(uuid.uuid4()), iid, cid, cuid)
                )

        print("--- Seeding Complete Successfully ---")
        conn.commit()

    except Exception as e:
        print(f"Error during seeding: {e}")
        conn.rollback()
    finally:
        cur.close()
        conn.close()

if __name__ == "__main__":
    seed_data()
