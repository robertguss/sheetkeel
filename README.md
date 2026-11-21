# sheetkeel

> **Blazing-fast, single-binary Rust Excel (`.xlsx`/`.xls`/`.xlsb`/`.ods`) Swiss-army CLI** — inspect messy workbooks, forward-fill merged cells (`--unmerge`), normalize headers, export to CSV/TSV/JSONL/Parquet/SQLite, pack formatted multi-tab workbooks, and diff spreadsheets (DX + AX).

Part of the [Keel Data Stack](https://github.com/robertguss/keel-stack).

## Why `sheetkeel`?

Analysts and engineers constantly receive multi-tab Excel workbooks with title banners, merged category cells, inconsistent column headers, and leading-zero IDs. `sheetkeel` handles all of it in milliseconds with zero Python/openpyxl/pandas overhead:

- **`inspect`**: Sheet dimensions, inferred column types, formula counts, sample rows, and **merged-cell warnings** (`--format auto|human|agent|json`).
- **`export`**: Export one sheet or **all sheets** to **CSV, TSV, JSONL, native ZSTD Parquet, or multi-table SQLite** — with `--unmerge` (forward-fills merged rectangles), `--clean-headers` (`snake_case` + deduplication), and `--header-row <N>`.
- **`pack`**: Turn CSV, TSV, JSONL, or SQLite `.db` tables into a polished multi-tab `.xlsx` workbook with bold frozen headers, autofilter, typed numbers/booleans, leading-zero text preservation, and auto-fitted column widths.
- **`diff`**: Sheet-by-sheet structural and cell-level diff between two workbooks (`--fail-on-diff` for CI/verification).
- **Bundled `SKILL.md`**: `sheetkeel skill --install .agents/skills/sheetkeel`.

## Install

```bash
# Via Cargo
cargo install sheetkeel

# Via uv / pip (prebuilt wheel)
uv tool install sheetkeel

# Or via the Keel Data Stack installer
curl -fsSL https://raw.githubusercontent.com/robertguss/keel-stack/main/scripts/install.sh | bash -s -- sheetkeel
```

## Quick Start

```bash
# 1. Inspect a workbook before touching it
sheetkeel inspect registrar_roster.xlsx --header-row 2

# 2. Clean merged cells + headers and export to native Parquet or SQLite
sheetkeel export registrar_roster.xlsx --sheet "Fall 2026" --header-row 2 --unmerge --clean-headers --out roster.parquet
sheetkeel export registrar_roster.xlsx --unmerge --clean-headers --out registrar.db

# 3. Scrub FERPA PII right after export using maskkeel
maskkeel mask roster.parquet dev_roster.parquet --require-salt

# 4. Pack CSV/JSONL/SQLite tables into a formatted multi-tab .xlsx
sheetkeel pack students.csv courses.jsonl --out report.xlsx

# 5. Diff two revisions of a spreadsheet
sheetkeel diff roster_v1.xlsx roster_v2.xlsx --fail-on-diff
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
