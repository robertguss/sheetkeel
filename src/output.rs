use crate::diff::DiffReport;
use crate::error::SheetkeelError;
use crate::export::ExportReport;
use crate::pack::PackReport;
use crate::workbook::InspectReport;
use clap::ValueEnum;
use owo_colors::OwoColorize;
use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Auto,
    Human,
    Agent,
    Json,
}

impl OutputFormat {
    pub fn resolve(self) -> Self {
        match self {
            Self::Auto => {
                if std::io::stdout().is_terminal() {
                    Self::Human
                } else {
                    Self::Agent
                }
            }
            other => other,
        }
    }
}

pub fn print_error(err: &SheetkeelError, fmt: OutputFormat) {
    match fmt.resolve() {
        OutputFormat::Json => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&err.to_envelope())
                    .unwrap_or_else(|_| format!("{err}"))
            );
        }
        OutputFormat::Agent => {
            eprintln!("ERROR [{}]: {}", err.code(), err);
            if !err.fix_hint().is_empty() {
                eprintln!("HINT: {}", err.fix_hint());
            }
        }
        OutputFormat::Human | OutputFormat::Auto => {
            eprintln!(
                "{} [{}] {}",
                "error:".red().bold(),
                err.code().yellow(),
                err
            );
            if !err.fix_hint().is_empty() {
                eprintln!("  {} {}", "hint:".cyan().bold(), err.fix_hint());
            }
        }
    }
}

pub fn print_inspect(report: &InspectReport, fmt: OutputFormat) {
    match fmt.resolve() {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Agent => {
            println!(
                "# sheetkeel inspect | workbook=`{}` | sheets={}",
                report.path, report.sheet_count
            );
            for s in &report.sheets {
                println!(
                    "\n## Sheet `{}` | data_rows={} | cols={} | merged_regions={} | formulas={}",
                    s.name,
                    s.data_rows,
                    s.columns.len(),
                    s.merged_regions,
                    s.formula_cells
                );
                for w in &s.warnings {
                    println!("- WARN: {w}");
                }
                if !s.columns.is_empty() {
                    println!("| column | type | non_null | nulls |");
                    println!("|---|---|---:|---:|");
                    for c in &s.columns {
                        println!(
                            "| {} | {} | {} | {} |",
                            c.name, c.inferred_type, c.non_null_count, c.null_count
                        );
                    }
                }
            }
        }
        OutputFormat::Human | OutputFormat::Auto => {
            println!(
                "{} {} ({} sheet(s))",
                "sheetkeel inspect".cyan().bold(),
                report.path.bold(),
                report.sheet_count
            );
            for s in &report.sheets {
                println!(
                    "\n  {} {} ({} rows × {} cols, {} merged, {} formulas)",
                    "▸".cyan().bold(),
                    s.name.bold(),
                    s.data_rows,
                    s.columns.len(),
                    s.merged_regions,
                    s.formula_cells
                );
                for w in &s.warnings {
                    println!("    {} {}", "!".yellow().bold(), w);
                }
                for c in &s.columns {
                    println!(
                        "    • {}: {} ({} non-null, {} null)",
                        c.name.cyan(),
                        c.inferred_type.green(),
                        c.non_null_count,
                        c.null_count
                    );
                }
            }
        }
    }
}

pub fn print_export(report: &ExportReport, fmt: OutputFormat) {
    match fmt.resolve() {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Agent => {
            println!(
                "# sheetkeel export | format={} | sheets={}",
                report.format,
                report.exported.len()
            );
            println!("| sheet | rows | cols | target |");
            println!("|---|---:|---:|---|");
            for e in &report.exported {
                println!("| {} | {} | {} | `{}` |", e.sheet, e.rows, e.cols, e.target);
            }
        }
        OutputFormat::Human | OutputFormat::Auto => {
            println!(
                "{} Exported {} sheet(s) from {} ({})",
                "✓".green().bold(),
                report.exported.len(),
                report.workbook.bold(),
                report.format
            );
            for e in &report.exported {
                println!(
                    "  • {} ({} rows × {} cols) -> {}",
                    e.sheet.cyan(),
                    e.rows,
                    e.cols,
                    e.target.bold()
                );
            }
        }
    }
}

pub fn print_pack(report: &PackReport, fmt: OutputFormat) {
    match fmt.resolve() {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Agent => {
            println!(
                "# sheetkeel pack | output=`{}` | sheets={}",
                report.output,
                report.sheets.len()
            );
            println!("| sheet | rows | cols | source |");
            println!("|---|---:|---:|---|");
            for s in &report.sheets {
                println!("| {} | {} | {} | `{}` |", s.sheet, s.rows, s.cols, s.source);
            }
        }
        OutputFormat::Human | OutputFormat::Auto => {
            println!(
                "{} Packed {} sheet(s) -> {}",
                "✓".green().bold(),
                report.sheets.len(),
                report.output.bold()
            );
            for s in &report.sheets {
                println!(
                    "  • {} ({} rows × {} cols) <- {}",
                    s.sheet.cyan(),
                    s.rows,
                    s.cols,
                    s.source.dimmed()
                );
            }
        }
    }
}

pub fn print_diff(report: &DiffReport, fmt: OutputFormat) {
    match fmt.resolve() {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Agent => {
            println!(
                "# sheetkeel diff | identical={} | sheets_changed={} | cell_changes={}",
                report.identical, report.sheets_changed, report.total_cell_changes
            );
            println!("| sheet | status | old_rows | new_rows | cell_changes |");
            println!("|---|---|---:|---:|---:|");
            for s in &report.sheets {
                println!(
                    "| {} | {} | {} | {} | {} |",
                    s.sheet, s.status, s.old_rows, s.new_rows, s.cell_changes_count
                );
            }
            for s in &report.sheets {
                if !s.added_columns.is_empty() {
                    println!(
                        "- `{}` added columns: {}",
                        s.sheet,
                        s.added_columns.join(", ")
                    );
                }
                if !s.removed_columns.is_empty() {
                    println!(
                        "- `{}` removed columns: {}",
                        s.sheet,
                        s.removed_columns.join(", ")
                    );
                }
                for ch in &s.sample_changes {
                    println!(
                        "- `{}` row {} `{}`: `{}` -> `{}`",
                        s.sheet, ch.row, ch.column, ch.old_value, ch.new_value
                    );
                }
            }
        }
        OutputFormat::Human | OutputFormat::Auto => {
            if report.identical {
                println!("{} Workbooks are identical.", "✓".green().bold());
            } else {
                println!(
                    "{} {} sheet(s) changed, {} cell difference(s)",
                    "Δ".yellow().bold(),
                    report.sheets_changed,
                    report.total_cell_changes
                );
            }
            for s in &report.sheets {
                println!(
                    "  • {} [{}] ({} -> {} rows, {} cell diffs)",
                    s.sheet.cyan(),
                    s.status,
                    s.old_rows,
                    s.new_rows,
                    s.cell_changes_count
                );
                for ch in &s.sample_changes {
                    println!(
                        "      row {} {}: {} -> {}",
                        ch.row,
                        ch.column.bold(),
                        ch.old_value.red(),
                        ch.new_value.green()
                    );
                }
            }
        }
    }
}
