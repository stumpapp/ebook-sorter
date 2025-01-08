use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, ContentArrangement, Table};
use epub::doc::EpubDoc;
use eyre::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::{fs::File, io::BufReader, path::PathBuf};

use clap::{Parser, ValueEnum};
use walkdir::{DirEntry, WalkDir};

#[derive(ValueEnum, Clone, Default, Debug)]
enum PlaceStrategy {
    Copy,
    #[default]
    Move,
}

/// A program to organize your ebooks by extracting metadata from them.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The root directory where the program will search for ebooks. If not provided,
    /// the program will search for ebooks in the current directory.
    #[clap(short, long)]
    root: Option<PathBuf>,
    /// The directory where the program will store the ebooks. If not provided,
    /// the program will store the ebooks in root, if provided, or the current directory.
    #[clap(short, long)]
    output: Option<PathBuf>,
    /// The strategy to use when organizing the ebooks. If not provided, the program will
    /// move the ebooks.
    #[clap(short, long, default_value = "move")]
    strategy: PlaceStrategy,
}

#[derive(Debug, thiserror::Error)]
enum EbookSortError {
    #[error("Failed to parse file as an EPUB document")]
    InvalidEbook { path: PathBuf, error: String },
    #[error("Failed to perform IO operation: {0}")]
    IoError(std::io::Error),
}

fn main() -> Result<()> {
    let args = Args::parse();

    let root = match args.root {
        Some(root) => root,
        _ => std::env::current_dir()?,
    };
    let output = args.output.unwrap_or_else(|| root.clone());

    let mut errors = Vec::new();

    let total_files = WalkDir::new(&root)
        .min_depth(0)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension() == Some("epub".as_ref()))
        .count();
    let bar = ProgressBar::new(total_files as u64).with_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{wide_bar:.cyan/blue} {pos}/{len} {percent}%")?,
    );

    let walker = WalkDir::new(root).min_depth(0);

    for entry in walker.into_iter().filter_map(Result::ok) {
        let Some(extension) = entry.path().extension() else {
            continue;
        };

        if extension != "epub" {
            continue;
        }

        bar.set_message(format!(
            "{}",
            entry.path().file_name().unwrap().to_string_lossy()
        ));

        let book = match EpubDoc::new(entry.path()) {
            Ok(book) => book,
            Err(e) => {
                errors.push(EbookSortError::InvalidEbook {
                    path: entry.path().to_path_buf(),
                    error: e.to_string(),
                });
                bar.inc(1);
                continue;
            }
        };

        let creator = book
            .metadata
            .get("creator")
            .map(|c| c.join(", "))
            .unwrap_or("Unsorted".to_string());

        let author_dir = output.join(creator);
        if !author_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&author_dir) {
                errors.push(EbookSortError::IoError(e));
                bar.inc(1);
                continue;
            }
        }
        let filename = format_book(&book, &entry);
        let destination = author_dir.join(filename);

        match args.strategy {
            PlaceStrategy::Copy => {
                if let Err(e) = std::fs::copy(entry.path(), &destination) {
                    errors.push(EbookSortError::IoError(e));
                    continue;
                }
            }
            PlaceStrategy::Move => {
                if let Err(e) = std::fs::rename(entry.path(), &destination) {
                    errors.push(EbookSortError::IoError(e));
                    continue;
                }
            }
        }

        bar.inc(1);
    }

    bar.finish();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Error", "Path"]);

    for error in errors {
        match error {
            EbookSortError::InvalidEbook { path, error } => {
                table.add_row(vec![error, path.to_string_lossy().to_string()]);
            }
            EbookSortError::IoError(e) => {
                table.add_row(vec![e.to_string(), String::default()]);
            }
        }
    }

    println!("{table}");

    Ok(())
}

fn format_book(book: &EpubDoc<BufReader<File>>, entry: &DirEntry) -> String {
    match book.metadata.get("title").and_then(|t| t.first().cloned()) {
        Some(title) => format!("{}.epub", title.trim()),
        _ => entry.file_name().to_string_lossy().trim().to_string(),
    }
}
