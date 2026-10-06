//! Complete LCOV line and branch records; imported data remains diagnostic.
use super::{normalize_source_path, LcovFile, LcovReport};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;

#[derive(Default)]
struct Record {
    path: String,
    file: LcovFile,
    branches: BTreeMap<(usize, String, String), u64>,
    totals: BTreeMap<String, usize>,
}

pub(super) fn parse(text: &str) -> Result<LcovReport> {
    let root = std::env::current_dir().context("resolve LCOV source directory")?;
    let mut report = LcovReport::default();
    let mut record: Option<Record> = None;
    let mut branches = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("TN:") {
            if record.is_some() {
                bail!("incomplete LCOV at line {line_number}: test name before end_of_record");
            }
        } else if let Some(path) = line.strip_prefix("SF:") {
            if record.is_some() || path.trim().is_empty() {
                bail!("incomplete LCOV at line {line_number}: missing end_of_record or empty SF");
            }
            record = Some(Record {
                path: normalize_source_path(&root, path),
                ..Record::default()
            });
        } else if line == "end_of_record" {
            let completed = record.take().context("LCOV end_of_record before SF")?;
            completed.validate_totals()?;
            for (branch, count) in completed.branches {
                let total: &mut u64 = branches
                    .entry((completed.path.clone(), branch))
                    .or_default();
                *total = total
                    .checked_add(count)
                    .context("LCOV branch count overflow")?;
            }
            let file = report.files.entry(completed.path).or_default();
            for (line, count) in completed.file.lines {
                let total = file.lines.entry(line).or_default();
                *total = total
                    .checked_add(count)
                    .context("LCOV line count overflow")?;
            }
        } else {
            let current = record.as_mut().context("LCOV data record before SF")?;
            let (kind, data) = line.split_once(':').context("malformed LCOV record")?;
            current
                .read(kind, data)
                .with_context(|| format!("LCOV line {line_number}"))?;
        }
    }
    if record.is_some() {
        bail!("incomplete LCOV: missing final end_of_record");
    }
    report.total_lines = report.files.values().map(|file| file.lines.len()).sum();
    report.covered_lines = report
        .files
        .values()
        .map(|file| file.lines.values().filter(|count| **count > 0).count())
        .sum();
    report.total_branches = branches.len();
    report.covered_branches = branches.values().filter(|count| **count > 0).count();
    if report.total_lines == 0 {
        bail!("LCOV report contains no instrumented source lines");
    }
    Ok(report)
}

impl Record {
    fn read(&mut self, kind: &str, data: &str) -> Result<()> {
        match kind {
            "DA" => {
                let fields: Vec<_> = data.split(',').collect();
                if !(2..=3).contains(&fields.len()) || fields.last() == Some(&"") {
                    bail!("malformed LCOV DA record");
                }
                let line = positive_line(fields[0])?;
                let count = fields[1].parse::<u64>().context("invalid LCOV DA count")?;
                if self.file.lines.insert(line, count).is_some() {
                    bail!("duplicate LCOV DA line in one source record");
                }
            }
            "BRDA" => {
                let fields: Vec<_> = data.split(',').collect();
                if fields.len() != 4 || fields.iter().any(|field| field.is_empty()) {
                    bail!("malformed LCOV BRDA record");
                }
                let key = (
                    positive_line(fields[0])?,
                    fields[1].into(),
                    fields[2].into(),
                );
                let count = if fields[3] == "-" {
                    0
                } else {
                    fields[3]
                        .parse::<u64>()
                        .context("invalid LCOV branch count")?
                };
                if self.branches.insert(key, count).is_some() {
                    bail!("duplicate LCOV branch in one source record");
                }
            }
            "LF" | "LH" | "BRF" | "BRH" | "FNF" | "FNH" => {
                let count = data
                    .parse::<usize>()
                    .context("invalid LCOV summary count")?;
                if self.totals.insert(kind.into(), count).is_some() {
                    bail!("duplicate LCOV summary count");
                }
            }
            // Function/version metadata does not establish line or branch hits.
            "FN" | "FNDA" | "FNL" | "FNA" | "VER" if !data.is_empty() => {}
            _ => bail!("unsupported or malformed LCOV record {kind}"),
        }
        Ok(())
    }

    fn validate_totals(&self) -> Result<()> {
        let lines = self.file.lines.len();
        let hit_lines = self.file.lines.values().filter(|count| **count > 0).count();
        // LF/LH count instrumented lines, and LLVM (cargo-llvm-cov) derives them
        // from its line summary, which can name lines that carry no DA record.
        // So a declared total may exceed the captured DA records, but never fall
        // below them, and a record that declares lines but captured none is cut.
        let declared_lines = self.totals.get("LF").copied();
        let declared_hits = self.totals.get("LH").copied();
        if declared_lines.is_some_and(|declared| declared < lines) {
            bail!("incomplete LCOV: LF is below the captured DA records");
        }
        if declared_lines.is_some_and(|declared| declared > 0) && lines == 0 {
            bail!("incomplete LCOV: LF declares lines but no DA records were captured");
        }
        if declared_hits.is_some_and(|declared| declared < hit_lines) {
            bail!("incomplete LCOV: LH is below the captured hit DA records");
        }
        if let (Some(found), Some(hit)) = (declared_lines, declared_hits) {
            if hit > found {
                bail!("invalid LCOV line summary: LH exceeds LF");
            }
        }
        for (kind, actual) in [
            ("BRF", self.branches.len()),
            (
                "BRH",
                self.branches.values().filter(|count| **count > 0).count(),
            ),
        ] {
            if self
                .totals
                .get(kind)
                .is_some_and(|declared| *declared != actual)
            {
                bail!("incomplete LCOV: {kind} does not match captured records");
            }
        }
        if let (Some(found), Some(hit)) = (self.totals.get("FNF"), self.totals.get("FNH")) {
            if hit > found {
                bail!("invalid LCOV function summary");
            }
        }
        Ok(())
    }
}

fn positive_line(value: &str) -> Result<usize> {
    let line = value.parse::<usize>().context("invalid LCOV source line")?;
    if line == 0 {
        bail!("LCOV source line must be positive");
    }
    Ok(line)
}
