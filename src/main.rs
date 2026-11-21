mod diff;
mod error;
mod export;
mod output;
mod pack;
mod skill;
mod workbook;

use crate::diff::diff_workbooks;
use crate::export::{ExportFormat, export_workbook};
use crate::output::{
    OutputFormat, print_diff, print_error, print_export, print_inspect, print_pack,
};
use crate::pack::pack_to_xlsx;
use crate::workbook::{ReadOptions, inspect_workbook};
use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "sheetkeel",
    version,
    about = "Blazing-fast Rust Excel (.xlsx/.xls/.xlsb/.ods) Swiss-army CLI: inspect, unmerge, export, pack, and diff spreadsheets (DX + AX)."
)]
struct Cli {
    /// Output format: auto (human on TTY, agent markdown when piped), human, agent, or json.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Auto)]
    format: OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Inspect an Excel/ODS workbook: sheets, dimensions, headers, inferred types, merged cells, and formula counts.
    Inspect {
        /// Path to .xlsx, .xlsm, .xls, .xlsb, or .ods workbook.
        workbook: String,

        /// 1-based header row index (default: 1).
        #[arg(long, default_value_t = 1)]
        header_row: usize,

        /// Preview normalized snake_case header names.
        #[arg(long)]
        clean_headers: bool,

        /// Forward-fill merged cells before inspecting column non-null counts.
        #[arg(long)]
        unmerge: bool,
    },

    /// Export one sheet or all sheets to CSV, TSV, JSONL, Parquet, or SQLite.
    Export {
        /// Path to input workbook (.xlsx, .xlsm, .xls, .xlsb, .ods).
        workbook: String,

        /// Output file path (for single sheet or .db SQLite) or output directory (for multi-sheet exports).
        #[arg(long, short = 'o')]
        out: String,

        /// Export only this sheet name (case-insensitive). Omit to export all sheets.
        #[arg(long)]
        sheet: Option<String>,

        /// Target format (auto-detected from --out extension if omitted).
        #[arg(long, value_enum)]
        to: Option<ExportFormat>,

        /// 1-based header row index (default: 1).
        #[arg(long, default_value_t = 1)]
        header_row: usize,

        /// Normalize messy Excel headers to clean snake_case and deduplicate collisions.
        #[arg(long)]
        clean_headers: bool,

        /// Forward-fill merged cell ranges across the merged rectangle (essential for grouped Registrar/Finance sheets!).
        #[arg(long)]
        unmerge: bool,

        /// Keep completely empty rows instead of dropping them.
        #[arg(long)]
        keep_empty_rows: bool,
    },

    /// Pack one or more CSV, TSV, JSONL, or SQLite files into a formatted multi-tab .xlsx workbook.
    Pack {
        /// One or more input files (.csv, .tsv, .jsonl, .db/.sqlite).
        #[arg(required = true)]
        inputs: Vec<String>,

        /// Output .xlsx workbook path.
        #[arg(long, short = 'o')]
        out: String,

        /// Disable frozen header row + autofilter styling.
        #[arg(long)]
        no_freeze: bool,
    },

    /// Compare two Excel/ODS workbooks sheet-by-sheet for added/removed sheets, column shifts, and cell changes.
    Diff {
        /// Baseline workbook path.
        old: String,

        /// Candidate workbook path.
        new: String,

        /// 1-based header row index (default: 1).
        #[arg(long, default_value_t = 1)]
        header_row: usize,

        /// Forward-fill merged cells before diffing.
        #[arg(long)]
        unmerge: bool,

        /// Maximum cell changes to include per sheet (default: 20).
        #[arg(long, default_value_t = 20)]
        max_samples: usize,

        /// Exit with code 1 if any sheet or cell differences are found.
        #[arg(long)]
        fail_on_diff: bool,
    },

    /// Print the bundled AI agent SKILL.md or install it into a directory (e.g. .agents/skills/sheetkeel).
    Skill {
        /// Directory where SKILL.md should be written.
        #[arg(long)]
        install: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fmt = cli.format;
    match run(cli) {
        Ok(code) => code,
        Err(err) => {
            print_error(&err, fmt);
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> crate::error::Result<ExitCode> {
    let fmt = cli.format;
    match cli.command {
        Commands::Inspect {
            workbook,
            header_row,
            clean_headers,
            unmerge,
        } => {
            let opts = ReadOptions {
                header_row: header_row.max(1),
                clean_headers,
                unmerge,
                drop_empty_rows: true,
            };
            let report = inspect_workbook(&workbook, &opts)?;
            print_inspect(&report, fmt);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Export {
            workbook,
            out,
            sheet,
            to,
            header_row,
            clean_headers,
            unmerge,
            keep_empty_rows,
        } => {
            let opts = ReadOptions {
                header_row: header_row.max(1),
                clean_headers,
                unmerge,
                drop_empty_rows: !keep_empty_rows,
            };
            let report = export_workbook(&workbook, &out, sheet.as_deref(), to, &opts)?;
            print_export(&report, fmt);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Pack {
            inputs,
            out,
            no_freeze,
        } => {
            let report = pack_to_xlsx(&inputs, &out, !no_freeze)?;
            print_pack(&report, fmt);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Diff {
            old,
            new,
            header_row,
            unmerge,
            max_samples,
            fail_on_diff,
        } => {
            let opts = ReadOptions {
                header_row: header_row.max(1),
                clean_headers: false,
                unmerge,
                drop_empty_rows: true,
            };
            let report = diff_workbooks(&old, &new, &opts, max_samples)?;
            print_diff(&report, fmt);
            if fail_on_diff && !report.identical {
                Ok(ExitCode::from(1))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }

        Commands::Skill { install } => {
            if let Some(dir) = install {
                let path = skill::install_skill(&dir)?;
                match fmt.resolve() {
                    OutputFormat::Json => {
                        println!("{}", serde_json::json!({ "ok": true, "installed": path }));
                    }
                    _ => println!("Installed sheetkeel SKILL.md to {path}"),
                }
            } else {
                print!("{}", skill::SKILL_MD);
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
