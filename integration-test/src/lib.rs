/*
    allocscope  -  a memory tracking tool
    Copyright (C) 2023  Matt Kimball

    This program is free software: you can redistribute it and/or modify it
    under the terms of the GNU General Public License as published by the
    Free Software Foundation, either version 3 of the License, or (at your
    option) any later version.

    This program is distributed in the hope that it will be useful, but
    WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License
    for more details.

    You should have received a copy of the GNU General Public License along
    with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use regex::Regex;
use std::error::Error;
use std::process;

// Data from one line of text output from a trace report.
#[derive(Clone, Debug)]
pub struct ReportLine {
    // The peak bytes allocated by that stack entry and its children.
    pub bytes: String,

    // The total number of blocks allocated.
    pub blocks: String,

    // The number of unfreed blocks.
    pub leaks: String,

    // ASCII art for the position of this stack entry in the tree.
    pub tree: String,

    // The name and offset of the function containing the code for this
    // stack entry.
    pub name: String,
}

// Compile a single source file, using the appropriate compiler based on the
// file extension.  Return the filename of the resulting binary, which can be
// traced for a test case.
pub fn compile_source(filename: &str) -> Result<String, Box<dyn Error>> {
    let source_path = format!("{}/{}", std::env::var("TEST_TRACEE_PATH")?, filename);

    let period_offset = filename
        .find('.')
        .ok_or("no extension in source filename")?;
    let basename = &filename[..period_offset];
    let extension = &filename[period_offset..];

    // Generate an output filename based on the source filename and our PID.
    let binary_path = format!("/tmp/{}-{}", basename, process::id());

    let mut command = match extension {
        ".c" => {
            // C source code.
            let mut command = process::Command::new(std::env::var("CC")?);
            command.args([&source_path, "-lpthread", "-o", &binary_path]);
            command
        }
        ".cc" => {
            // C++ source code.
            let mut command = process::Command::new(std::env::var("CXX")?);
            command.args([&source_path, "-lpthread", "-o", &binary_path]);
            command
        }
        ".rs" => {
            // Rust source code.
            let mut command = process::Command::new(std::env::var("RUSTC")?);
            command.args([&source_path, "-o", &binary_path]);
            command
        }
        _ => panic!("Unknown extension: {}", extension),
    };

    let compiler_status = command.spawn()?.wait()?;
    assert_eq!(compiler_status.code(), Some(0));

    Ok(binary_path)
}

// Given a string representing a binary to trace, use the version of
// allocscope-trace under test to generate a trace file.
pub fn perform_trace(command: &str) -> Result<String, Box<dyn Error>> {
    let trace_path = format!("{}.atrace", command);

    let trace_status = process::Command::new(std::env::var("TEST_ALLOCSCOPE_TRACE")?)
        .args(["-o", &trace_path, &command])
        .spawn()?
        .wait()?;
    assert_eq!(trace_status.code(), Some(0));

    Ok(trace_path)
}

// Given a line of text output from allocscope-view, parse the text into a
// ReportLine struct.
pub fn parse_report_line(line: &str) -> Result<ReportLine, Box<dyn Error>> {
    let re = Regex::new(r"([0-9A-Za-z]+) +([0-9A-Za-z]+) +([0-9A-Za-z]+) ([-+| ]+)(.+)")?;
    let caps = re.captures(line).ok_or("no captures")?;

    let bytes = caps.get(1).ok_or("missing bytes")?.as_str().to_string();
    let blocks = caps.get(2).ok_or("missing blocks")?.as_str().to_string();
    let leaks = caps.get(3).ok_or("missing leaks")?.as_str().to_string();
    let tree = caps.get(4).ok_or("missing tree")?.as_str().to_string();
    let name = caps
        .get(5)
        .ok_or("missing function name")?
        .as_str()
        .to_string();
    let line = ReportLine {
        blocks,
        bytes,
        leaks,
        tree,
        name,
    };

    Ok(line)
}

// Given the filename of a trace file, return a vector of ReportLines
// representing the output of the version of allocscope-view under test.
pub fn view_trace(atrace_path: &str) -> Result<Vec<ReportLine>, Box<dyn Error>> {
    let output = process::Command::new(std::env::var("TEST_ALLOCSCOPE_VIEW")?)
        .arg(&atrace_path)
        .output()?;
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout)?;

    let mut found_table = false;
    let mut report_lines = Vec::new();
    for line in stdout.split("\n") {
        if found_table {
            if line.len() > 0 {
                report_lines.push(parse_report_line(line)?);
            }
        } else if line.find("BYTES BLOCK LEAKS") == Some(0) {
            found_table = true;
        }
    }

    Ok(report_lines)
}

// Find the index of the top leaf node in the report's output.  Assuming that
// the report is sorted by peak allocation size, this will be the allocation
// call responsible for the highest individual memory usage.
pub fn find_top_leaf_index(trace: &Vec<ReportLine>) -> Option<usize> {
    if trace.len() == 0 {
        return None;
    }

    let mut ix = 0;
    loop {
        let next_ix = ix + 1;
        if next_ix >= trace.len() {
            break;
        }

        if trace[next_ix].tree.len() <= trace[ix].tree.len() {
            break;
        }

        ix += 1;
    }

    Some(ix)
}

// Build a source file and perform a trace on the resulting binary.  Return
// the output of that trace report as a vector of ReportLine structs.
pub fn build_and_trace(source_filename: &str) -> Result<Vec<ReportLine>, Box<dyn Error>> {
    let binary_path = compile_source(source_filename)?;

    let trace_result = perform_trace(&binary_path);
    std::fs::remove_file(&binary_path)?;
    let trace_path = trace_result?;

    let view_result = view_trace(&trace_path);
    std::fs::remove_file(&trace_path)?;

    view_result
}

// Build a source file, perform a trace, and return the resulting ReportLine
// for the top leaf stackentry in the report.
pub fn build_and_get_leaf(source_filename: &str) -> Result<ReportLine, Box<dyn Error>> {
    let trace = build_and_trace(source_filename)?;

    let leaf_ix = find_top_leaf_index(&trace).ok_or("no top leaf")?;
    let leaf = &trace[leaf_ix];
    println!("{} leaf: {:?}", source_filename, leaf);

    Ok(leaf.clone())
}

// Build a source file, perform a trace, and return a ReportLine for the
// stack entry matching a particular function name in the stack of the top
// leaf entry of the report.
pub fn build_and_get_named(
    source_filename: &str,
    function_name: &str,
) -> Result<ReportLine, Box<dyn Error>> {
    let trace = build_and_trace(source_filename)?;

    let mut leaf_ix = find_top_leaf_index(&trace).ok_or("no top leaf")?;
    loop {
        let leaf = &trace[leaf_ix];
        if leaf.name.contains(function_name) {
            println!("{} function: {:?}", source_filename, leaf);

            return Ok(leaf.clone());
        }

        if leaf_ix == 0 {
            break;
        }
        leaf_ix -= 1;
    }

    Err("no matching function".into())
}
