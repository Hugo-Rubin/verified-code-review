//! CSV report output.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    pub label: String,
    pub count: u64,
}

/// Write `rows` to `path` as CSV, replacing any existing file.
pub fn write_report(path: &Path, rows: &[Row]) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);

    writeln!(out, "id,label,count")?;
    for row in rows {
        writeln!(out, "{},{},{}", row.id, row.label, row.count)?;
    }

    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_header_and_every_row() {
        let dir = std::env::temp_dir().join("c07-report-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.csv");

        let rows = vec![
            Row {
                id: 1,
                label: "a".into(),
                count: 10,
            },
            Row {
                id: 2,
                label: "b".into(),
                count: 20,
            },
        ];
        write_report(&path, &rows).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("id,label,count\n"));
        assert!(text.contains("1,a,10\n"));
        assert!(text.contains("2,b,20\n"));

        std::fs::remove_file(&path).ok();
    }
}
