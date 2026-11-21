use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use rust_xlsxwriter::Workbook;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

fn sheetkeel_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("sheetkeel"))
}

fn build_messy_registrar_xlsx(path: &std::path::Path) {
    let mut wb = Workbook::new();

    // Sheet 1: Banner row + merged department column + messy headers + formula
    let ws1 = wb.add_worksheet();
    ws1.set_name("Fall 2026 Roster").unwrap();
    ws1.write_string(0, 0, "Westminster Registrar Export")
        .unwrap();
    ws1.write_string(1, 0, "Program Track").unwrap();
    ws1.write_string(1, 1, "Student ID #").unwrap();
    ws1.write_string(1, 2, "Credits Earned").unwrap();
    ws1.write_string(1, 3, "GPA (4.0)").unwrap();

    // Merged cells A3:A4 = "MDiv"
    ws1.merge_range(2, 0, 3, 0, "MDiv", &rust_xlsxwriter::Format::new())
        .unwrap();
    ws1.write_string(2, 1, "001001").unwrap();
    ws1.write_number(2, 2, 42.0).unwrap();
    ws1.write_number(2, 3, 3.92).unwrap();

    ws1.write_string(3, 1, "001002").unwrap();
    ws1.write_number(3, 2, 30.0).unwrap();
    ws1.write_number(3, 3, 3.65).unwrap();

    ws1.write_string(4, 0, "MAR").unwrap();
    ws1.write_string(4, 1, "001003").unwrap();
    ws1.write_formula(4, 2, "=SUM(C3:C4)").unwrap();
    ws1.write_number(4, 3, 3.80).unwrap();

    // Sheet 2: Courses
    let ws2 = wb.add_worksheet();
    ws2.set_name("Courses").unwrap();
    ws2.write_string(0, 0, "Course Code").unwrap();
    ws2.write_string(0, 1, "Title").unwrap();
    ws2.write_string(1, 0, "NT101").unwrap();
    ws2.write_string(1, 1, "Greek Exegesis").unwrap();

    wb.save(path).unwrap();
}

#[test]
fn test_inspect_and_unmerge_export_csv_parquet_sqlite() {
    let dir = tempdir().unwrap();
    let xlsx = dir.path().join("registrar.xlsx");
    build_messy_registrar_xlsx(&xlsx);

    // 1. Inspect (header-row 2 catches merged region warning + formula count!)
    let insp_out = sheetkeel_cmd()
        .args([
            "inspect",
            xlsx.to_str().unwrap(),
            "--header-row",
            "2",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let insp: Value = serde_json::from_slice(&insp_out).unwrap();
    assert_eq!(insp["sheet_count"], 2);
    let s0 = &insp["sheets"][0];
    assert_eq!(s0["name"], "Fall 2026 Roster");
    assert_eq!(s0["merged_regions"], 1);
    assert_eq!(s0["formula_cells"], 1);
    assert_eq!(s0["data_rows"], 3);

    // 2. Export with --unmerge --clean-headers to CSV
    let csv_out = dir.path().join("roster.csv");
    sheetkeel_cmd()
        .args([
            "export",
            xlsx.to_str().unwrap(),
            "--sheet",
            "Fall 2026 Roster",
            "--header-row",
            "2",
            "--unmerge",
            "--clean-headers",
            "--out",
            csv_out.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();

    let csv_text = fs::read_to_string(&csv_out).unwrap();
    let lines: Vec<&str> = csv_text.lines().collect();
    assert_eq!(lines[0], "program_track,student_id,credits_earned,gpa_4_0");
    assert!(lines[1].starts_with("MDiv,001001,42,3.92"));
    // Row 2 MUST have forward-filled "MDiv" thanks to --unmerge!
    assert!(
        lines[2].starts_with("MDiv,001002,30,3.65"),
        "Expected --unmerge to forward-fill MDiv on row 2, got: {}",
        lines[2]
    );

    // 3. Export single sheet to native Parquet
    let pq_out = dir.path().join("roster.parquet");
    sheetkeel_cmd()
        .args([
            "export",
            xlsx.to_str().unwrap(),
            "--sheet",
            "Fall 2026 Roster",
            "--header-row",
            "2",
            "--unmerge",
            "--clean-headers",
            "--out",
            pq_out.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(fs::metadata(&pq_out).unwrap().len() > 100);

    // 4. Export ALL sheets into a single SQLite database
    let db_out = dir.path().join("registrar.db");
    sheetkeel_cmd()
        .args([
            "export",
            xlsx.to_str().unwrap(),
            "--clean-headers",
            "--out",
            db_out.to_str().unwrap(),
        ])
        .assert()
        .success();
    let conn = Connection::open(&db_out).unwrap();
    let course_cnt: i64 = conn
        .query_row("SELECT COUNT(*) FROM courses", [], |r| r.get(0))
        .unwrap();
    assert_eq!(course_cnt, 1);
}

#[test]
fn test_pack_csv_jsonl_and_diff_workbooks() {
    let dir = tempdir().unwrap();
    let c1 = dir.path().join("students.csv");
    let c2 = dir.path().join("students_v2.csv");
    let j1 = dir.path().join("courses.jsonl");
    let wb1 = dir.path().join("v1.xlsx");
    let wb2 = dir.path().join("v2.xlsx");

    fs::write(
        &c1,
        "sis_user_id,name,gpa\n001001,Alice,3.90\n001002,Bob,3.40\n",
    )
    .unwrap();
    fs::write(
        &c2,
        "sis_user_id,name,gpa\n001001,Alice,3.95\n001002,Bob,3.40\n",
    )
    .unwrap();
    fs::write(&j1, "{\"code\":\"NT101\",\"credits\":3}\n").unwrap();

    // Pack v1 (2 tabs: students + courses)
    sheetkeel_cmd()
        .args([
            "pack",
            c1.to_str().unwrap(),
            j1.to_str().unwrap(),
            "--out",
            wb1.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();

    // Pack v2 (rename c2 to students.csv in subdir so sheet names match!)
    let v2_dir = dir.path().join("v2src");
    fs::create_dir_all(&v2_dir).unwrap();
    let c2_named = v2_dir.join("students.csv");
    fs::copy(&c2, &c2_named).unwrap();

    sheetkeel_cmd()
        .args([
            "pack",
            c2_named.to_str().unwrap(),
            j1.to_str().unwrap(),
            "--out",
            wb2.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Diff v1 vs v1 -> identical (exit 0)
    sheetkeel_cmd()
        .args([
            "diff",
            wb1.to_str().unwrap(),
            wb1.to_str().unwrap(),
            "--fail-on-diff",
        ])
        .assert()
        .success();

    // Diff v1 vs v2 -> detects GPA change 3.9 -> 3.95 and exits 1 with --fail-on-diff!
    let diff_out = sheetkeel_cmd()
        .args([
            "diff",
            wb1.to_str().unwrap(),
            wb2.to_str().unwrap(),
            "--fail-on-diff",
            "--format",
            "json",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let d: Value = serde_json::from_slice(&diff_out).unwrap();
    assert_eq!(d["identical"], false);
    assert_eq!(d["sheets_changed"], 1);
    assert_eq!(d["total_cell_changes"], 1);

    // Leading zero preserved on pack -> export roundtrip!
    let rt_csv = dir.path().join("roundtrip.csv");
    sheetkeel_cmd()
        .args([
            "export",
            wb1.to_str().unwrap(),
            "--sheet",
            "students",
            "--out",
            rt_csv.to_str().unwrap(),
        ])
        .assert()
        .success();
    let rt = fs::read_to_string(&rt_csv).unwrap();
    assert!(rt.contains("001001,Alice"));

    // Skill command & --install
    sheetkeel_cmd()
        .args(["skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name: sheetkeel"));
    let sdir = dir.path().join(".agents/skills/sheetkeel");
    sheetkeel_cmd()
        .args(["skill", "--install", sdir.to_str().unwrap()])
        .assert()
        .success();
    assert!(sdir.join("SKILL.md").exists());
}
