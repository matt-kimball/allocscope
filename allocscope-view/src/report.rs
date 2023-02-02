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

use crate::rows;
use crate::trace;
use std::collections;
use std::error::Error;

// Format a large value for printing in a five column space, using
// an appropriate suffix.
pub fn format_table_value(value: u64, divisor: u64) -> String {
    if value < 99999 {
        format!("{:5}", value)
    } else if value / divisor < 9999 {
        format!("{:4}k", value / divisor)
    } else if value / divisor / divisor < 9999 {
        format!("{:4}M", value / divisor / divisor)
    } else if value / divisor / divisor / divisor < 9999 {
        format!("{:4}G", value / divisor / divisor / divisor)
    } else if value / divisor / divisor / divisor / divisor < 9999 {
        format!("{:4}T", value / divisor / divisor / divisor / divisor)
    } else {
        format!(
            "{:4}P",
            value / divisor / divisor / divisor / divisor / divisor
        )
    }
}

// Format the name of a function, using ASCII to indicate the call tree.
pub fn format_function_tree_row(
    collapsed: Option<&collections::HashSet<trace::StackEntryId>>,
    entry: &rows::StackEntryRow,
) -> String {
    let mut indent = String::new();
    for depth in 0..entry.depth {
        indent = indent
            + if depth == entry.depth - 1 {
                "+-"
            } else {
                if entry.final_child_of_depth[depth] {
                    "  "
                } else {
                    "| "
                }
            };
    }

    let function_name = if entry.function.len() > 0 {
        if entry.offset > 0 {
            format!("{} + 0x{:x}", entry.function, entry.offset)
        } else {
            entry.function.clone()
        }
    } else {
        format!("0x{:x}", entry.address)
    };

    format!(
        "{}{} {}",
        indent,
        if entry.has_children {
            let entry_collapsed = match collapsed {
                Some(hashset) => hashset.contains(&entry.id),
                None => false,
            };

            if entry_collapsed {
                "#"
            } else {
                "|"
            }
        } else {
            "-"
        },
        function_name,
    )
}

// Generate a report of allocations to stdout, in a text format suitable for
// redirecting to a text file or being piped to another command.
pub fn generate_report(trace: trace::Trace) -> Result<(), Box<dyn Error>> {
    let mut transaction = trace::Transaction::new(&trace)?;

    let row_count = rows::count_rows(&mut transaction, None)?;
    let rows =
        rows::iter_stackentry_rows(&mut transaction, rows::SortMode::Bytes, None, 0, row_count)?;

    println!("allocscope {} memory report", env!("CARGO_PKG_VERSION"));
    println!("https://support.mkimball.net/");
    println!("");
    println!("BYTES BLOCK LEAKS   Function");
    for entry in rows {
        let function = format_function_tree_row(None, &entry);
        println!(
            "{} {} {} {}",
            format_table_value(entry.maximum_size, 1024),
            format_table_value(entry.total_blocks, 1000),
            format_table_value(entry.unfreed_blocks, 1000),
            function,
        );
    }

    Ok(())
}
