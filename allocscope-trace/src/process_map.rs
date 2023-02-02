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

use std::error::Error;
use std::io::BufRead;

// An entry for a mmap-ed region in the traced process.
#[derive(Debug)]
pub struct ProcessMapEntry {
    // The memory address at which the mapping starts.
    pub begin: u64,

    // The memory address at which the mapping ends.
    pub end: u64,

    // The offset within the mapped file for this mapping.
    pub offset: u64,

    // The filename of the mapped file.
    pub filename: Option<String>,
}

// A list of all the mmap-ed regions of a traced process.
#[derive(Debug)]
pub struct ProcessMap {
    // The list of mmap-ed regions.
    pub entries: Vec<ProcessMapEntry>,
}

impl ProcessMap {
    // Construct a new ProcessMap for the current state of a process, using
    // the /proc filesystem entry for that process.
    pub fn new(pid: u32) -> Result<ProcessMap, Box<dyn Error>> {
        let mut entries = Vec::<ProcessMapEntry>::new();

        let maps_file = std::fs::File::open(format!("/proc/{}/maps", pid))?;
        for line_result in std::io::BufReader::new(maps_file).lines() {
            let line = line_result?;
            let mut tokens = line.split_whitespace();
            let range = tokens.next().ok_or("missing address range")?;
            let mut split = range.split('-');
            let begin = u64::from_str_radix(split.next().ok_or("missing range start")?, 16)?;
            let end = u64::from_str_radix(split.next().ok_or("missing range end")?, 16)?;

            let mut tokens = tokens.skip(1);
            let offset = u64::from_str_radix(tokens.next().ok_or("missing mapping offset")?, 16)?;
            let mut tokens = tokens.skip(2);

            let mut filename: Option<String> = None;
            match tokens.next() {
                Some(name) => filename = Some(name.to_string()),
                None => (),
            }

            entries.push(ProcessMapEntry {
                begin,
                end,
                offset,
                filename,
            });
        }
        Ok(ProcessMap { entries })
    }

    // Find the mmap region containing a particular address in the traced
    // process.
    pub fn entry_for_address(&self, address: u64) -> Option<&ProcessMapEntry> {
        for entry in &self.entries {
            if address >= entry.begin && address < entry.end {
                return Some(entry);
            }
        }

        None
    }
}
