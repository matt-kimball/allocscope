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

use crate::trace;
use cplus_demangle;
use rustc_demangle;
use std::collections;
use std::error::Error;

// The column by which we should sort rows generated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SortMode {
    // Sort by the sequence the events occur in the trace file.
    None,

    // Sort by max concurrent bytes allocated.
    Bytes,

    // Sort by total blocks allocated.
    Blocks,

    // Sort by number of unfreed blocks allocated.
    Leaks,
}

// A row generated for display, representing a stack frame location.
pub struct StackEntryRow {
    // The identifier of the stack entry in the trace.
    pub id: trace::StackEntryId,

    // The number of generations of parent frames this row has.
    pub depth: usize,

    // Entries in this vector are true if this row is the final descendent of
    // the corresponding ancestor.
    pub final_child_of_depth: Vec<bool>,

    // True if this row has children.
    pub has_children: bool,

    // The address of instruction pointer for this stack frame.
    pub address: u64,

    // The name of the function for this stack frame.
    pub function: String,

    // The offset from the start of the function.
    pub offset: u64,

    // The maximum concurrent allocation size, in bytes of this stack frame
    // and its descendents.
    pub maximum_size: u64,

    // The total number of blocks allocated by this stack frame and its
    // descendents.
    pub total_blocks: u64,

    // The total number of blocks allocated by this stack frame and its
    // descendents which were never freed.
    pub unfreed_blocks: u64,
}

// Bookkeeping information used while generating rows to track information
// which is expensive to recompute.
struct WorkingEntry {
    // The stack entry queued for generation.
    stackentry: trace::StackEntry,

    // The number of generations of parent frames this stack entry has.
    depth: usize,

    // Entries in this vector are true if this row is the final descendent of
    // the corresponding ancestor.
    final_child_of_depth: Vec<bool>,
}

impl StackEntryRow {
    // Complete generation of a row for display, using our scratch information
    // and the open transaction to the database.
    fn new(
        transaction: &mut trace::Transaction,
        entry: &WorkingEntry,
        has_children: bool,
    ) -> Option<StackEntryRow> {
        let location = transaction.location(entry.stackentry.location)?;
        let mut maximum_size = 0;
        let mut total_blocks = 0;
        let mut unfreed_blocks = 0;
        if let Some(summary) = transaction.summary(entry.stackentry.id) {
            maximum_size = summary.maximum_total;
            total_blocks = summary.alloc_count;
            unfreed_blocks = summary.alloc_count - summary.free_count;
        }
        if total_blocks == 0 {
            return None;
        }

        Some(StackEntryRow {
            id: entry.stackentry.id,
            depth: entry.depth,
            final_child_of_depth: entry.final_child_of_depth.clone(),
            has_children: has_children,
            address: location.address,
            function: location.function?,
            offset: location.offset?,
            maximum_size,
            total_blocks,
            unfreed_blocks,
        })
    }
}

// Sort stack entries by one of our sort modes.
pub fn sort_stackentries(
    transaction: &mut trace::Transaction,
    stackentries: &mut dyn Iterator<Item = trace::StackEntry>,
    sort_mode: SortMode,
) -> Result<Vec<trace::StackEntry>, Box<dyn Error>> {
    // Filter out any entries without a summary.
    let mut vec: Vec<trace::StackEntry> = stackentries
        .filter(|entry| transaction.summary(entry.id).is_some())
        .collect();

    match sort_mode {
        SortMode::Bytes => vec.sort_by(|a, b| {
            let summary_a = transaction.summary(a.id).unwrap();
            let summary_b = transaction.summary(b.id).unwrap();
            summary_b
                .maximum_total
                .partial_cmp(&summary_a.maximum_total)
                .unwrap()
        }),

        SortMode::Blocks => vec.sort_by(|a, b| {
            let summary_a = transaction.summary(a.id).unwrap();
            let summary_b = transaction.summary(b.id).unwrap();
            summary_b
                .alloc_count
                .partial_cmp(&summary_a.alloc_count)
                .unwrap()
        }),

        SortMode::Leaks => vec.sort_by(|a, b| {
            let summary_a = transaction.summary(a.id).unwrap();
            let summary_b = transaction.summary(b.id).unwrap();
            let leaks_a = summary_a.alloc_count - summary_a.free_count;
            let leaks_b = summary_b.alloc_count - summary_b.free_count;
            leaks_b.partial_cmp(&leaks_a).unwrap()
        }),

        _ => {}
    }

    Ok(vec)
}

// Interpret any function name as potentially a C++ or Rust function and
// demangle if possible.
fn demangle_function_name(name: &str) -> String {
    let mut function: String = name.to_string();
    function = match cplus_demangle::demangle(&function) {
        Ok(function) => function,
        Err(_) => function,
    };
    function = rustc_demangle::demangle(&function).to_string();

    function
}

// Given a set of stack entries, find the set of all ancestors of those stack
// entries.  This is useful for performance reasons, because we can
// efficiently skip rows if we know no decendents are collapsed.
fn gather_ancestors(
    transaction: &mut trace::Transaction,
    entries: Option<&collections::HashSet<trace::StackEntryId>>,
) -> Result<collections::HashSet<trace::StackEntryId>, Box<dyn Error>> {
    let mut ancestors: collections::HashSet<trace::StackEntryId> = collections::HashSet::new();
    if entries.is_none() {
        return Ok(ancestors);
    }

    for stackentry in entries.unwrap().iter() {
        let mut ancestor: Option<trace::StackEntryId> = Some(*stackentry);

        while ancestor.is_some() {
            let ancestor_id = ancestor.unwrap();
            ancestors.insert(ancestor_id);
            ancestor = transaction
                .stackentry(ancestor_id)
                .ok_or("missing stackentry")?
                .next;
        }
    }

    Ok(ancestors)
}

// Generate some number of rows for display from an open transaction to the database.
pub fn iter_stackentry_rows(
    transaction: &mut trace::Transaction,
    sort_mode: SortMode,
    collapsed: Option<&collections::HashSet<trace::StackEntryId>>,
    skip_rows: usize,
    max_rows: usize,
) -> Result<Vec<StackEntryRow>, Box<dyn Error>> {
    let collapsed_ancestors = gather_ancestors(transaction, collapsed)?;

    let mut rows = Vec::new();
    let mut entries: collections::VecDeque<WorkingEntry> = collections::VecDeque::new();
    let roots = transaction.root_stackentries()?;
    for stackentry in sort_stackentries(transaction, &mut roots.into_iter(), sort_mode)? {
        entries.push_back(WorkingEntry {
            stackentry,
            depth: 0,
            final_child_of_depth: Vec::new(),
        })
    }

    let mut skipped = 0;
    while rows.len() < max_rows {
        if let Some(entry) = entries.pop_front() {
            let descendent_count = transaction.descendent_count(entry.stackentry.id)? as usize;

            let mut row = StackEntryRow::new(transaction, &entry, descendent_count > 0)
                .ok_or("failure retrieving entry row")?;
            if skipped < skip_rows {
                skipped += 1;
            } else {
                row.function = demangle_function_name(&row.function);
                rows.push(row);
            }

            let entry_collapsed = match collapsed {
                Some(hashset) => hashset.contains(&entry.stackentry.id),
                None => false,
            };

            if !entry_collapsed {
                // If we know no children are collapsed, we can use the
                // precomputed descendent count to skip rows, which speeds
                // up large traces to make the UI usable.
                if skipped + descendent_count < skip_rows
                    && !collapsed_ancestors.contains(&entry.stackentry.id)
                {
                    skipped += descendent_count;
                } else {
                    let children = transaction.get_stackentry_children(entry.stackentry.id)?;

                    let mut final_child = true;
                    // We are reversing here because we are pushing entries on
                    // the *front* of the working vector.
                    for child in
                        sort_stackentries(transaction, &mut children.into_iter(), sort_mode)?
                            .into_iter()
                            .rev()
                    {
                        let mut final_child_of_depth = entry.final_child_of_depth.clone();
                        final_child_of_depth.push(final_child);
                        final_child = false;

                        entries.push_front(WorkingEntry {
                            stackentry: child,
                            depth: entry.depth + 1,
                            final_child_of_depth: final_child_of_depth,
                        });
                    }
                }
            }
        } else {
            break;
        }
    }

    Ok(rows)
}

// Count all the rows which can be potentially be displayed.  Used by
// the ncurses UI to know how many rows to skip to get to the end of
// the trace.
pub fn count_rows(
    transaction: &mut trace::Transaction,
    collapsed: Option<&collections::HashSet<trace::StackEntryId>>,
) -> Result<usize, Box<dyn Error>> {
    let collapsed_ancestors = gather_ancestors(transaction, collapsed)?;
    let mut count = 0;

    let mut entries: collections::VecDeque<WorkingEntry> = collections::VecDeque::new();
    for stackentry in transaction.root_stackentries()? {
        entries.push_back(WorkingEntry {
            stackentry,
            depth: 0,
            final_child_of_depth: Vec::new(),
        })
    }

    while let Some(entry) = entries.pop_front() {
        if let Some(_) = StackEntryRow::new(transaction, &entry, false) {
            count += 1;
        } else {
            continue;
        }

        let entry_collapsed = match collapsed {
            Some(hashset) => hashset.contains(&entry.stackentry.id),
            None => false,
        };

        if !entry_collapsed {
            let descendent_count = transaction.descendent_count(entry.stackentry.id)? as usize;

            if !collapsed_ancestors.contains(&entry.stackentry.id) {
                count += descendent_count;
            } else {
                let children = transaction.get_stackentry_children(entry.stackentry.id)?;
                for ix in (0..children.len()).rev() {
                    entries.push_front(WorkingEntry {
                        stackentry: children[ix].clone(),
                        depth: entry.depth + 1,
                        final_child_of_depth: Vec::new(),
                    });
                }
            }
        }
    }

    Ok(count)
}
