use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, ContentArrangement, Table};
use epub::doc::{EpubDoc, MetadataItem};
use eyre::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::{fmt, fs::File, io::BufReader, path::PathBuf};

use clap::{Parser, ValueEnum};
use walkdir::{DirEntry, WalkDir};

#[derive(ValueEnum, Clone, Default, Debug)]
enum PlaceStrategy {
    Copy,
    #[default]
    Move,
}

impl fmt::Display for PlaceStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaceStrategy::Copy => write!(f, "Copy"),
            PlaceStrategy::Move => write!(f, "Move"),
        }
    }
}

/// A program to organize your ebooks by extracting metadata from them.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Whether to run the program in dry run mode. In dry run mode, the program will
    /// not perform any file operations, but will still print what it would do.
    #[clap(short, long, action = clap::ArgAction::SetTrue, default_value_t = false)]
    dry_run: bool,
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

#[derive(Debug)]
struct BookParams {
    filename: String,
    creator: String,
    series: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let root = match args.root {
        Some(root) => root,
        _ => std::env::current_dir()?,
    };
    let output = args.output.unwrap_or_else(|| root.clone());

    let dry_run = args.dry_run;

    let mut errors = Vec::new();
    let mut dry_run_operations = Vec::new();

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

        let BookParams {
            filename,
            creator,
            series,
        } = extract_metadata(&book, &entry);

        let author_dir = output.join(creator);
        if !author_dir.exists() && !dry_run {
            if let Err(e) = std::fs::create_dir_all(&author_dir) {
                errors.push(EbookSortError::IoError(e));
                bar.inc(1);
                continue;
            }
        }

        let series_dir = series.as_ref().map(|s| author_dir.join(s));
        match (series_dir.as_ref(), dry_run) {
            (Some(series_path), false) if !series_path.exists() => {
                if let Err(e) = std::fs::create_dir_all(series_path) {
                    errors.push(EbookSortError::IoError(e));
                    bar.inc(1);
                    continue;
                }
            }
            _ => {}
        }

        let destination = if let Some(series_path) = series_dir {
            series_path.join(filename)
        } else {
            author_dir.join(filename)
        };

        if dry_run {
            dry_run_operations.push((format!("{:?}", entry.path()), format!("{:?}", destination)));
            bar.inc(1);
            continue;
        }

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

    if dry_run {
        bar.set_message("Dry run completed. No files were modified.");
        print_dry_run_operations(dry_run_operations, args.strategy);
        return Ok(());
    }

    if errors.is_empty() {
        bar.set_message("All ebooks processed successfully.");
        return Ok(());
    }

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

fn filter_metadata_item_for_any_of(item: &&epub::doc::MetadataItem, properties: &[&str]) -> bool {
    properties
        .iter()
        .any(|&property| item.property == property || item.refinement(property).is_some())
}

fn filter_metadata_item_for_property(item: &&epub::doc::MetadataItem, property: &str) -> bool {
    item.property == property || item.refinement(property).is_some()
}

fn extract_creator(metadata: &[MetadataItem]) -> Option<String> {
    let items_with_creator: Vec<&MetadataItem> = metadata
        .iter()
        .filter(|item| filter_metadata_item_for_property(item, "creator"))
        .collect();

    let creators = items_with_creator
        .iter()
        .filter_map(|item| {
            if item.property == "creator" {
                Some(item.value.clone())
            } else {
                item.refined
                    .iter()
                    .find(|r| r.property == "creator")
                    .map(|r| r.value.clone())
            }
        })
        .collect::<Vec<_>>();

    if creators.is_empty() {
        None
    } else {
        Some(creators.join(", "))
    }
}

const VALID_SERIES_PROPERTIES: [&str; 3] = ["series", "calibre:series", "belongs-to-collection"];

fn extract_series(metadata: &[MetadataItem]) -> Option<String> {
    let items_with_series: Vec<&MetadataItem> = metadata
        .iter()
        .filter(|item| filter_metadata_item_for_any_of(item, &VALID_SERIES_PROPERTIES))
        .collect();

    let series = items_with_series
        .iter()
        .filter_map(|item| {
            if VALID_SERIES_PROPERTIES.contains(&item.property.as_str()) {
                Some(item.value.clone())
            } else {
                item.refined
                    .iter()
                    .find(|r| VALID_SERIES_PROPERTIES.contains(&r.property.as_str()))
                    .map(|r| r.value.clone())
            }
        })
        .collect::<Vec<_>>();

    series.first().cloned().map(|s| s.trim().to_string())
}

fn extract_metadata(book: &EpubDoc<BufReader<File>>, entry: &DirEntry) -> BookParams {
    let filename = book
        .get_title()
        .map_or_else(
            || entry.file_name().to_string_lossy().trim().to_string(),
            |s| format!("{}.epub", s.trim()),
        )
        .replace('/', "-") // Replace slashes to avoid directory issues
        .replace('\\', "-"); // Replace backslashes to avoid directory issues

    let creator = extract_creator(&book.metadata).unwrap_or_else(|| "Unsorted".to_string());

    let series = extract_series(&book.metadata);

    BookParams {
        filename,
        creator,
        series,
    }
}

fn print_dry_run_operations(
    operations: Vec<(impl fmt::Display, impl fmt::Display)>,
    strategy: PlaceStrategy,
) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Source", "Destination", "Strategy"]);

    for (source, destination) in operations {
        table.add_row(vec![
            source.to_string(),
            destination.to_string(),
            strategy.to_string(),
        ]);
    }

    println!("{table}");
}
