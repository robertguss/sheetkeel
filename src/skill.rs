use crate::error::{Result, SheetkeelError};
use std::path::Path;

pub const SKILL_MD: &str = r#"---
name: sheetkeel
description: Blazing-fast, single-binary Rust Excel (.xlsx/.xls/.xlsb/.ods) Swiss-army CLI for inspecting workbooks, forward-filling merged cells (--unmerge), normalizing headers, exporting to CSV/TSV/JSONL/Parquet/SQLite, packing multi-tab .xlsx workbooks, and diffing spreadsheets.
---

# sheetkeel — Rust Excel & Spreadsheet Swiss-Army CLI

`sheetkeel` replaces slow Python `openpyxl`/`pandas` spreadsheet scripts with a single fast binary designed for both humans (DX) and AI agents (AX).

## Typical Workflows (Higher-Ed / Registrar / Analytics / PII Pipelines)

### 1. Inspect an Unfamiliar Workbook First
```bash
sheetkeel inspect registrar_roster.xlsx --format agent
```
- Lists all sheets, row/col dimensions, inferred column types, formula counts, and **merged-cell warnings** before you touch a single row.
- If the workbook has title/banner rows above the table header, pass `--header-row 3`.

### 2. Clean & Export Sheets to CSV, JSONL, Parquet, or SQLite
```bash
# Export one sheet with merged cells forward-filled & headers normalized to snake_case
sheetkeel export registrar_roster.xlsx --sheet "Fall 2026" --unmerge --clean-headers --out roster.csv

# Export ALL sheets as native ZSTD-compressed Parquet files in a folder
sheetkeel export financial_aid.xlsx --unmerge --clean-headers --to parquet --out ./parquet_sheets/

# Export ALL sheets into a single queryable SQLite database (one table per sheet!)
sheetkeel export admissions.xlsx --unmerge --clean-headers --out admissions.db
```

### 3. Pair with `maskkeel`, `statkeel`, `datakeel`, `loadkeel`
```bash
# Excel -> clean Parquet -> deterministic FERPA PII scrub -> profile!
sheetkeel export raw_students.xlsx --unmerge --clean-headers --out raw_students.parquet
maskkeel mask raw_students.parquet dev_students.parquet --require-salt
statkeel profile dev_students.parquet --format agent
```

### 4. Pack CSV / JSONL / SQLite Tables into a Polished Multi-Tab `.xlsx`
```bash
sheetkeel pack students.csv courses.jsonl --out report.xlsx
sheetkeel pack lake.db --out full_export.xlsx
```
- Preserves leading-zero IDs (e.g. `001001`) as text, formats bold frozen header rows, enables autofilters, and auto-fits column widths.

### 5. Diff Two Spreadsheet Revisions
```bash
sheetkeel diff roster_v1.xlsx roster_v2.xlsx --format agent --fail-on-diff
```

## Output Modes & Exit Codes
- `--format auto|human|agent|json` (auto-switches to clean Markdown `agent` format when stdout is piped).
- Exit `0`: success (or identical workbooks on `diff`).
- Exit `1`: `diff --fail-on-diff` detected differences.
- Exit `2`: workbook, I/O, export, or config error.
"#;

pub fn install_skill(target_dir: &str) -> Result<String> {
    let dir = Path::new(target_dir);
    std::fs::create_dir_all(dir).map_err(|e| SheetkeelError::io(e, target_dir))?;
    let file = dir.join("SKILL.md");
    std::fs::write(&file, SKILL_MD)
        .map_err(|e| SheetkeelError::io(e, &file.display().to_string()))?;
    Ok(file.display().to_string())
}
