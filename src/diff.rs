use crate::error::Result;
use crate::workbook::{ReadOptions, read_sheets};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct CellChange {
    pub row: usize,
    pub column: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SheetDiff {
    pub sheet: String,
    pub status: String, // "unchanged" | "changed" | "added" | "removed"
    pub old_rows: usize,
    pub new_rows: usize,
    pub added_columns: Vec<String>,
    pub removed_columns: Vec<String>,
    pub cell_changes_count: usize,
    pub sample_changes: Vec<CellChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub identical: bool,
    pub old_path: String,
    pub new_path: String,
    pub sheets_changed: usize,
    pub total_cell_changes: usize,
    pub sheets: Vec<SheetDiff>,
}

pub fn diff_workbooks(
    old_path: &str,
    new_path: &str,
    opts: &ReadOptions,
    max_samples: usize,
) -> Result<DiffReport> {
    let old_tables = read_sheets(old_path, None, opts)?;
    let new_tables = read_sheets(new_path, None, opts)?;

    let old_map: HashMap<String, _> = old_tables
        .into_iter()
        .map(|t| (t.name.clone(), t))
        .collect();
    let new_map: HashMap<String, _> = new_tables
        .into_iter()
        .map(|t| (t.name.clone(), t))
        .collect();

    let mut all_names: Vec<String> = old_map
        .keys()
        .chain(new_map.keys())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    all_names.sort();

    let mut sheets = Vec::new();
    let mut sheets_changed = 0usize;
    let mut total_cell_changes = 0usize;

    for name in all_names {
        match (old_map.get(&name), new_map.get(&name)) {
            (Some(ot), None) => {
                sheets_changed += 1;
                sheets.push(SheetDiff {
                    sheet: name,
                    status: "removed".to_string(),
                    old_rows: ot.rows.len(),
                    new_rows: 0,
                    added_columns: Vec::new(),
                    removed_columns: ot.headers.clone(),
                    cell_changes_count: 0,
                    sample_changes: Vec::new(),
                });
            }
            (None, Some(nt)) => {
                sheets_changed += 1;
                sheets.push(SheetDiff {
                    sheet: name,
                    status: "added".to_string(),
                    old_rows: 0,
                    new_rows: nt.rows.len(),
                    added_columns: nt.headers.clone(),
                    removed_columns: Vec::new(),
                    cell_changes_count: 0,
                    sample_changes: Vec::new(),
                });
            }
            (Some(ot), Some(nt)) => {
                let oset: HashSet<&str> = ot.headers.iter().map(|s| s.as_str()).collect();
                let nset: HashSet<&str> = nt.headers.iter().map(|s| s.as_str()).collect();
                let added_columns: Vec<String> = nt
                    .headers
                    .iter()
                    .filter(|h| !oset.contains(h.as_str()))
                    .cloned()
                    .collect();
                let removed_columns: Vec<String> = ot
                    .headers
                    .iter()
                    .filter(|h| !nset.contains(h.as_str()))
                    .cloned()
                    .collect();

                let common_cols: Vec<(String, usize, usize)> = ot
                    .headers
                    .iter()
                    .enumerate()
                    .filter_map(|(oi, h)| {
                        nt.headers
                            .iter()
                            .position(|nh| nh == h)
                            .map(|ni| (h.clone(), oi, ni))
                    })
                    .collect();

                let min_rows = ot.rows.len().min(nt.rows.len());
                let mut cell_changes_count = 0usize;
                let mut sample_changes = Vec::new();

                for ri in 0..min_rows {
                    for (cname, oi, ni) in &common_cols {
                        let ov = ot.rows[ri]
                            .get(*oi)
                            .map(|c| c.to_csv_string())
                            .unwrap_or_default();
                        let nv = nt.rows[ri]
                            .get(*ni)
                            .map(|c| c.to_csv_string())
                            .unwrap_or_default();
                        if ov != nv {
                            cell_changes_count += 1;
                            if sample_changes.len() < max_samples {
                                sample_changes.push(CellChange {
                                    row: ri + opts.header_row + 1,
                                    column: cname.clone(),
                                    old_value: ov,
                                    new_value: nv,
                                });
                            }
                        }
                    }
                }

                let changed = !added_columns.is_empty()
                    || !removed_columns.is_empty()
                    || ot.rows.len() != nt.rows.len()
                    || cell_changes_count > 0;
                if changed {
                    sheets_changed += 1;
                }
                total_cell_changes += cell_changes_count;

                sheets.push(SheetDiff {
                    sheet: name,
                    status: if changed { "changed" } else { "unchanged" }.to_string(),
                    old_rows: ot.rows.len(),
                    new_rows: nt.rows.len(),
                    added_columns,
                    removed_columns,
                    cell_changes_count,
                    sample_changes,
                });
            }
            (None, None) => unreachable!(),
        }
    }

    Ok(DiffReport {
        identical: sheets_changed == 0,
        old_path: old_path.to_string(),
        new_path: new_path.to_string(),
        sheets_changed,
        total_cell_changes,
        sheets,
    })
}
