# ebook-sorter

> Dislaimer: This was a scrappy script I put together mostly for a one-off task for a friend with a massive collection of unsorted ebooks. Use at your own risk.

A Rust command-line tool to sort and organize ebook files into a structured directory based on their embedded metadata.

## Usage

You can see the available options by running:

```bash
cargo run -- --help
```

At the time of writing, this should output something like:

```bash
A program to organize your ebooks by extracting metadata from them

Usage: ebook-sorter [OPTIONS]

Options:
  -d, --dry-run              Whether to run the program in dry run mode. In dry run mode, the program will not perform any file operations, but will still print what it would do
  -r, --root <ROOT>          The root directory where the program will search for ebooks. If not provided, the program will search for ebooks in the current directory
  -o, --output <OUTPUT>      The directory where the program will store the ebooks. If not provided, the program will store the ebooks in root, if provided, or the current directory
  -s, --strategy <STRATEGY>  The strategy to use when organizing the ebooks. If not provided, the program will move the ebooks [default: move] [possible values: copy, move]
  -h, --help                 Print help
  -V, --version              Print version
```

### Placement

The tool supports two strategies for placing sorted ebooks:

- **Move**: Moves the ebook files to the sorted directory, removing them from their original location.
- **Copy**: Copies the ebook files to the sorted directory, leaving the original files intact

### Dry Run

To test the sorting without making any changes to your files, you can use the `--dry-run` flag. This will simulate the sorting process and display what would happen without actually moving or copying any files.

## Notes

If you have a really large collection of books, it might be useful to run a dry run first and output the stdout to a file for easier review. Something like this:

```bash
cargo run -- --root '/path/to/your/unsorted/ebooks' --output '/path/to/your/sorted/ebooks' --strategy 'copy' --dry-run > output.txt
```
