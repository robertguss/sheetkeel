use crate::error::{Result, SheetkeelError};
use crate::workbook::CellValue;
use indexmap::IndexSet;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use rust_xlsxwriter::{Color, Format, Workbook};
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct PackedSheet {
    pub sheet: String,
    pub source: String,
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackReport {
    pub ok: bool,
    pub output: String,
    pub sheets: Vec<PackedSheet>,
}

struct InputTable {
    sheet_name: String,
    source: String,
    headers: Vec<String>,
    rows: Vec<Vec<CellValue>>,
}

fn excel_sheet_name(raw: &str, existing: &mut IndexSet<String>) -> String {
    // Excel sheet names max 31 chars, cannot contain : \ / ? * [ ]
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            ':' | '\\' | '/' | '?' | '*' | '[' | ']' => '_',
            other => other,
        })
        .collect();
    let base: String = cleaned.trim().chars().take(28).collect();
    let base = if base.is_empty() {
        "Sheet".to_string()
    } else {
        base
    };
    if existing.insert(base.to_ascii_lowercase()) {
        return base;
    }
    for n in 2..999 {
        let cand = format!("{base}_{n}");
        if existing.insert(cand.to_ascii_lowercase()) {
            return cand;
        }
    }
    base
}

fn parse_cell_typed(raw: &str) -> CellValue {
    let t = raw.trim();
    if t.is_empty() {
        return CellValue::Null;
    }
    if t.eq_ignore_ascii_case("true") {
        return CellValue::Bool(true);
    }
    if t.eq_ignore_ascii_case("false") {
        return CellValue::Bool(false);
    }
    // Preserve leading-zero IDs (like "001001") as text!
    if !(t.len() > 1 && t.starts_with('0') && !t.starts_with("0.")) {
        if let Ok(i) = t.parse::<i64>() {
            return CellValue::Int(i);
        }
        if let Ok(f) = t.parse::<f64>()
            && f.is_finite()
        {
            return CellValue::Float(f);
        }
    }
    CellValue::Str(raw.to_string())
}

fn load_delimited(path: &str, delim: u8) -> Result<InputTable> {
    let sheet_name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Sheet")
        .to_string();
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .from_path(path)
        .map_err(|e| SheetkeelError::io(e, path))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| SheetkeelError::export(format!("CSV header error `{path}`: {e}"), ""))?
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| SheetkeelError::export(format!("CSV row error: {e}"), ""))?;
        rows.push(
            (0..headers.len())
                .map(|i| parse_cell_typed(rec.get(i).unwrap_or("")))
                .collect(),
        );
    }
    Ok(InputTable {
        sheet_name,
        source: path.to_string(),
        headers,
        rows,
    })
}

fn load_jsonl(path: &str) -> Result<InputTable> {
    let sheet_name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Sheet")
        .to_string();
    let file = File::open(path).map_err(|e| SheetkeelError::io(e, path))?;
    let rdr = BufReader::new(file);
    let mut cols: IndexSet<String> = IndexSet::new();
    let mut objs: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    for line in rdr.lines() {
        let line = line.map_err(|e| SheetkeelError::io(e, path))?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let val: serde_json::Value = serde_json::from_str(t)
            .map_err(|e| SheetkeelError::export(format!("JSONL parse error: {e}"), ""))?;
        if let Some(map) = val.as_object() {
            for k in map.keys() {
                cols.insert(k.clone());
            }
            objs.push(map.clone());
        }
    }
    let headers: Vec<String> = cols.into_iter().collect();
    let mut rows = Vec::with_capacity(objs.len());
    for map in objs {
        let row = headers
            .iter()
            .map(|h| match map.get(h) {
                None | Some(serde_json::Value::Null) => CellValue::Null,
                Some(serde_json::Value::Bool(b)) => CellValue::Bool(*b),
                Some(serde_json::Value::Number(n)) => {
                    if let Some(i) = n.as_i64() {
                        CellValue::Int(i)
                    } else if let Some(f) = n.as_f64() {
                        CellValue::Float(f)
                    } else {
                        CellValue::Str(n.to_string())
                    }
                }
                Some(serde_json::Value::String(s)) => CellValue::Str(s.clone()),
                Some(other) => CellValue::Str(other.to_string()),
            })
            .collect();
        rows.push(row);
    }
    Ok(InputTable {
        sheet_name,
        source: path.to_string(),
        headers,
        rows,
    })
}

fn load_sqlite(path: &str) -> Result<Vec<InputTable>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| SheetkeelError::export(format!("SQLite open failed `{path}`: {e}"), ""))?;
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
    let tnames: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| SheetkeelError::export(e.to_string(), ""))?
        .filter_map(|r| r.ok())
        .collect();

    let mut tables = Vec::new();
    for tbl in tnames {
        let qtbl = format!("\"{}\"", tbl.replace('"', "\"\""));
        let mut cstmt = conn
            .prepare(&format!("PRAGMA table_info({qtbl})"))
            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
        let headers: Vec<String> = cstmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?
            .filter_map(|r| r.ok())
            .collect();
        let col_q: Vec<String> = headers
            .iter()
            .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
            .collect();
        let mut q = conn
            .prepare(&format!("SELECT {} FROM {qtbl}", col_q.join(", ")))
            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
        let mut qrows = q
            .query([])
            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
        let mut rows = Vec::new();
        while let Some(r) = qrows
            .next()
            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?
        {
            let mut row = Vec::with_capacity(headers.len());
            for i in 0..headers.len() {
                let cv = match r.get_ref_unwrap(i) {
                    ValueRef::Null => CellValue::Null,
                    ValueRef::Integer(n) => CellValue::Int(n),
                    ValueRef::Real(f) => CellValue::Float(f),
                    ValueRef::Text(b) => CellValue::Str(String::from_utf8_lossy(b).into_owned()),
                    ValueRef::Blob(b) => CellValue::Str(format!("<{} bytes>", b.len())),
                };
                row.push(cv);
            }
            rows.push(row);
        }
        tables.push(InputTable {
            sheet_name: tbl.clone(),
            source: format!("{path}#{tbl}"),
            headers,
            rows,
        });
    }
    Ok(tables)
}

pub fn pack_to_xlsx(inputs: &[String], output: &str, freeze_header: bool) -> Result<PackReport> {
    if inputs.is_empty() {
        return Err(SheetkeelError::config(
            "No input files supplied to `pack`",
            "Pass one or more .csv, .tsv, .jsonl, or .db/.sqlite files.",
        ));
    }

    let mut tables = Vec::new();
    for inp in inputs {
        let low = inp.to_ascii_lowercase();
        if low.ends_with(".csv") {
            tables.push(load_delimited(inp, b',')?);
        } else if low.ends_with(".tsv") {
            tables.push(load_delimited(inp, b'\t')?);
        } else if low.ends_with(".jsonl") || low.ends_with(".ndjson") {
            tables.push(load_jsonl(inp)?);
        } else if low.ends_with(".db") || low.ends_with(".sqlite") || low.ends_with(".sqlite3") {
            tables.extend(load_sqlite(inp)?);
        } else {
            return Err(SheetkeelError::config(
                format!("Unsupported input `{inp}` for `pack`"),
                "Supported inputs: .csv, .tsv, .jsonl, .db/.sqlite",
            ));
        }
    }

    let mut wb = Workbook::new();
    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x1E293B))
        .set_font_color(Color::White);

    let mut used_names = IndexSet::new();
    let mut packed_sheets = Vec::new();

    for t in tables {
        let sname = excel_sheet_name(&t.sheet_name, &mut used_names);
        let ws = wb.add_worksheet();
        ws.set_name(&sname)
            .map_err(|e| SheetkeelError::export(format!("Worksheet name `{sname}`: {e}"), ""))?;

        for (c, h) in t.headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &header_fmt)
                .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
        }

        for (r_idx, row) in t.rows.iter().enumerate() {
            let er = (r_idx + 1) as u32;
            for (c_idx, cell) in row.iter().enumerate() {
                let ec = c_idx as u16;
                match cell {
                    CellValue::Null => {}
                    CellValue::Bool(b) => {
                        ws.write_boolean(er, ec, *b)
                            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
                    }
                    CellValue::Int(i) => {
                        ws.write_number(er, ec, *i as f64)
                            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
                    }
                    CellValue::Float(f) => {
                        ws.write_number(er, ec, *f)
                            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
                    }
                    CellValue::Str(s) => {
                        ws.write_string(er, ec, s)
                            .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
                    }
                }
            }
        }

        if freeze_header && !t.headers.is_empty() {
            ws.set_freeze_panes(1, 0)
                .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
            ws.autofilter(0, 0, t.rows.len() as u32, (t.headers.len() - 1) as u16)
                .map_err(|e| SheetkeelError::export(e.to_string(), ""))?;
        }
        ws.autofit();

        packed_sheets.push(PackedSheet {
            sheet: sname,
            source: t.source,
            rows: t.rows.len(),
            cols: t.headers.len(),
        });
    }

    wb.save(output).map_err(|e| SheetkeelError::io(e, output))?;

    Ok(PackReport {
        ok: true,
        output: output.to_string(),
        sheets: packed_sheets,
    })
}
