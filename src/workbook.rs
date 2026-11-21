use crate::error::{Result, SheetkeelError};
use calamine::{Data, Dimensions, Reader, Sheets, open_workbook_auto};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

#[derive(Debug, Clone)]
pub struct ReadOptions {
    /// 1-based header row index (default: 1).
    pub header_row: usize,
    /// Normalize header names to clean snake_case and deduplicate blanks/collisions.
    pub clean_headers: bool,
    /// Forward-fill merged cell ranges from the top-left cell across the merged rectangle.
    pub unmerge: bool,
    /// Drop completely empty rows.
    pub drop_empty_rows: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            header_row: 1,
            clean_headers: false,
            unmerge: false,
            drop_empty_rows: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Null => true,
            Self::Str(s) => s.trim().is_empty(),
            _ => false,
        }
    }

    pub fn to_csv_string(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 9e15 {
                    format!("{}", *f as i64)
                } else {
                    f.to_string()
                }
            }
            Self::Str(s) => s.clone(),
        }
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(b) => serde_json::Value::Bool(*b),
            Self::Int(i) => serde_json::json!(*i),
            Self::Float(f) => serde_json::json!(*f),
            Self::Str(s) => serde_json::Value::String(s.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SheetTable {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<CellValue>>,
    pub merged_region_count: usize,
    pub formula_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnSummary {
    pub name: String,
    pub inferred_type: String,
    pub non_null_count: usize,
    pub null_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SheetInspection {
    pub name: String,
    pub raw_rows: usize,
    pub raw_cols: usize,
    pub data_rows: usize,
    pub merged_regions: usize,
    pub formula_cells: usize,
    pub columns: Vec<ColumnSummary>,
    pub sample_rows: Vec<Vec<String>>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectReport {
    pub path: String,
    pub sheet_count: usize,
    pub sheets: Vec<SheetInspection>,
}

pub fn data_to_cell(d: &Data) -> CellValue {
    match d {
        Data::Empty => CellValue::Null,
        Data::Bool(b) => CellValue::Bool(*b),
        Data::Int(i) => CellValue::Int(*i),
        Data::Float(f) => {
            if f.fract() == 0.0 && *f >= (i64::MIN as f64) && *f <= (i64::MAX as f64) {
                CellValue::Int(*f as i64)
            } else {
                CellValue::Float(*f)
            }
        }
        Data::String(s) => CellValue::Str(s.clone()),
        Data::DateTime(dt) => {
            if let Some(ndt) = dt.as_datetime() {
                let t = ndt.time();
                if t == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap() {
                    CellValue::Str(ndt.date().format("%Y-%m-%d").to_string())
                } else {
                    CellValue::Str(ndt.format("%Y-%m-%dT%H:%M:%S").to_string())
                }
            } else {
                CellValue::Float(dt.as_f64())
            }
        }
        Data::DateTimeIso(s) | Data::DurationIso(s) => CellValue::Str(s.clone()),
        Data::Error(e) => CellValue::Str(format!("#{e:?}")),
    }
}

pub fn clean_header_name(raw: &str, idx: usize) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us && !out.is_empty() {
            out.push('_');
            prev_us = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        format!("col_{}", idx + 1)
    } else if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("c_{trimmed}")
    } else {
        trimmed
    }
}

pub fn sanitize_sheet_filename(name: &str) -> String {
    clean_header_name(name, 0)
}

fn deduplicate_headers(raw_headers: Vec<String>, clean: bool) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(raw_headers.len());
    for (i, h) in raw_headers.into_iter().enumerate() {
        let base = if clean {
            clean_header_name(&h, i)
        } else {
            let t = h.trim().to_string();
            if t.is_empty() {
                format!("col_{}", i + 1)
            } else {
                t
            }
        };
        let key = base.to_ascii_lowercase();
        let c = counts.entry(key).or_insert(0);
        *c += 1;
        if *c == 1 {
            out.push(base);
        } else {
            out.push(format!("{base}_{c}"));
        }
    }
    out
}

fn open_sheets(path: &str) -> Result<Sheets<BufReader<File>>> {
    open_workbook_auto(path).map_err(|e| {
        SheetkeelError::workbook(
            format!("Failed to open workbook `{path}`: {e}"),
            "Ensure the file exists and is a valid .xlsx, .xlsm, .xls, .xlsb, or .ods spreadsheet.",
        )
    })
}

fn get_merged_regions(wb: &mut Sheets<BufReader<File>>, sheet_name: &str) -> Vec<Dimensions> {
    if let Sheets::Xlsx(xlsx) = wb {
        xlsx.merge_cells_by_sheet_name(sheet_name)
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn read_sheets(
    path: &str,
    only_sheet: Option<&str>,
    opts: &ReadOptions,
) -> Result<Vec<SheetTable>> {
    let mut wb = open_sheets(path)?;
    let all_names = wb.sheet_names();
    let selected: Vec<String> = match only_sheet {
        Some(target) => {
            if let Some(found) = all_names.iter().find(|n| n.eq_ignore_ascii_case(target)) {
                vec![found.clone()]
            } else {
                return Err(SheetkeelError::workbook(
                    format!("Sheet `{target}` not found in `{path}`"),
                    format!("Available sheets: {}", all_names.join(", ")),
                ));
            }
        }
        None => all_names,
    };

    let mut tables = Vec::with_capacity(selected.len());
    for name in selected {
        let merged = get_merged_regions(&mut wb, &name);
        let formula_count = wb
            .worksheet_formula(&name)
            .map(|r| r.used_cells().count())
            .unwrap_or(0);
        let range = wb.worksheet_range(&name).map_err(|e| {
            SheetkeelError::workbook(format!("Failed reading sheet `{name}`: {e}"), "")
        })?;

        let (height, width) = range.get_size();
        let start = range.start().unwrap_or((0, 0));
        let mut grid: Vec<Vec<CellValue>> = vec![vec![CellValue::Null; width]; height];
        for (r, row) in range.rows().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                grid[r][c] = data_to_cell(cell);
            }
        }

        // Apply --unmerge forward-fill across merged rectangles!
        if opts.unmerge && !merged.is_empty() && height > 0 && width > 0 {
            for dim in &merged {
                let r0 = dim.start.0.saturating_sub(start.0) as usize;
                let c0 = dim.start.1.saturating_sub(start.1) as usize;
                let r1 = (dim.end.0.saturating_sub(start.0) as usize).min(height - 1);
                let c1 = (dim.end.1.saturating_sub(start.1) as usize).min(width - 1);
                if r0 < height && c0 < width {
                    let val = grid[r0][c0].clone();
                    if !val.is_empty() {
                        for row in grid.iter_mut().take(r1 + 1).skip(r0) {
                            for cell in row.iter_mut().take(c1 + 1).skip(c0) {
                                if cell.is_empty() {
                                    *cell = val.clone();
                                }
                            }
                        }
                    }
                }
            }
        }

        let header_idx = opts.header_row.saturating_sub(1);
        if height == 0 || width == 0 || header_idx >= height {
            tables.push(SheetTable {
                name,
                headers: Vec::new(),
                rows: Vec::new(),
                merged_region_count: merged.len(),
                formula_count,
            });
            continue;
        }

        let raw_headers: Vec<String> = grid[header_idx].iter().map(|c| c.to_csv_string()).collect();
        let headers = deduplicate_headers(raw_headers, opts.clean_headers);

        let mut rows = Vec::new();
        for row in grid.into_iter().skip(header_idx + 1) {
            if opts.drop_empty_rows && row.iter().all(|c| c.is_empty()) {
                continue;
            }
            rows.push(row);
        }

        tables.push(SheetTable {
            name,
            headers,
            rows,
            merged_region_count: merged.len(),
            formula_count,
        });
    }

    Ok(tables)
}

pub fn inspect_workbook(path: &str, opts: &ReadOptions) -> Result<InspectReport> {
    let mut wb = open_sheets(path)?;
    let names = wb.sheet_names();
    let mut raw_dims: HashMap<String, (usize, usize)> = HashMap::new();
    for n in &names {
        if let Ok(r) = wb.worksheet_range(n) {
            raw_dims.insert(n.clone(), r.get_size());
        }
    }
    drop(wb);

    let tables = read_sheets(path, None, opts)?;
    let mut sheets = Vec::with_capacity(tables.len());

    for t in tables {
        let (raw_rows, raw_cols) = raw_dims.get(&t.name).copied().unwrap_or((0, 0));
        let mut columns = Vec::with_capacity(t.headers.len());
        for (ci, h) in t.headers.iter().enumerate() {
            let mut non_null = 0usize;
            let mut nulls = 0usize;
            let mut saw_int = false;
            let mut saw_float = false;
            let mut saw_bool = false;
            let mut saw_str = false;

            for r in &t.rows {
                match r.get(ci).unwrap_or(&CellValue::Null) {
                    CellValue::Null => nulls += 1,
                    CellValue::Str(s) if s.trim().is_empty() => nulls += 1,
                    CellValue::Int(_) => {
                        non_null += 1;
                        saw_int = true;
                    }
                    CellValue::Float(_) => {
                        non_null += 1;
                        saw_float = true;
                    }
                    CellValue::Bool(_) => {
                        non_null += 1;
                        saw_bool = true;
                    }
                    CellValue::Str(_) => {
                        non_null += 1;
                        saw_str = true;
                    }
                }
            }
            let inferred_type = if non_null == 0 {
                "empty"
            } else if saw_str {
                "string"
            } else if saw_float {
                "float"
            } else if saw_int && !saw_bool {
                "int"
            } else if saw_bool && !saw_int {
                "bool"
            } else {
                "mixed"
            };
            columns.push(ColumnSummary {
                name: h.clone(),
                inferred_type: inferred_type.to_string(),
                non_null_count: non_null,
                null_count: nulls,
            });
        }

        let mut warnings = Vec::new();
        if t.merged_region_count > 0 && !opts.unmerge {
            warnings.push(format!(
                "{} merged cell region(s) detected — pass `--unmerge` when exporting to forward-fill merged values",
                t.merged_region_count
            ));
        }
        if t.headers.iter().any(|h| h.starts_with("col_")) {
            warnings.push(
                "One or more blank header cells detected — check `--header-row` if title/banner rows precede the table"
                    .to_string(),
            );
        }

        let sample_rows: Vec<Vec<String>> = t
            .rows
            .iter()
            .take(3)
            .map(|r| r.iter().map(|c| c.to_csv_string()).collect())
            .collect();

        sheets.push(SheetInspection {
            name: t.name,
            raw_rows,
            raw_cols,
            data_rows: t.rows.len(),
            merged_regions: t.merged_region_count,
            formula_cells: t.formula_count,
            columns,
            sample_rows,
            warnings,
        });
    }

    Ok(InspectReport {
        path: path.to_string(),
        sheet_count: sheets.len(),
        sheets,
    })
}
