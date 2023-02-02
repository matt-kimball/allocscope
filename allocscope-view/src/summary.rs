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
use std::error::Error;
use std::io;
use std::io::Write;
use std::time;

// Add an allocation to the summary for its stack entry and all ancestor
// stack entries.
fn add_to_summary(
    transaction: &mut trace::Transaction,
    bottom_id: trace::StackEntryId,
    allocation: bool,
    size: i64,
) -> Result<(), Box<dyn Error>> {
    let mut id = Some(bottom_id);
    while let Some(entry_id) = id {
        if let Some(stackentry) = transaction.stackentry(entry_id) {
            transaction.add_to_summary(entry_id, allocation, size)?;
            id = stackentry.next;
        } else {
            break;
        }
    }

    Ok(())
}

// Given an allocation, track its originating event as indexed by address,
// and add its size to the stack entry summaries.
fn process_alloc(
    transaction: &mut trace::Transaction,
    event: &trace::Event,
) -> Result<(), Box<dyn Error>> {
    transaction.insert_allocation_origin(event.address, event.id)?;
    if let Some(callstack_id) = event.callstack {
        if let Some(size) = event.size {
            add_to_summary(transaction, callstack_id, true, size as i64)?;
        }
    }

    Ok(())
}

// Given a free event, remove the originating event from the address index
// and update the stack entry summaries.
fn process_free(
    transaction: &mut trace::Transaction,
    event: &trace::Event,
) -> Result<(), Box<dyn Error>> {
    if let Some(alloc_event_id) = transaction.allocation_origin(event.address) {
        if let Some(alloc_event) = transaction.event(alloc_event_id) {
            if let Some(callstack_id) = alloc_event.callstack {
                if let Some(size) = alloc_event.size {
                    add_to_summary(transaction, callstack_id, false, -(size as i64))?;
                }
            }
        }
    }
    transaction.remove_allocation_origin(event.address)?;

    Ok(())
}

// Given a stack entry, increment the descendent count for all ancestors
// of that entry.
fn increment_descendent_counts(
    transaction: &mut trace::Transaction,
    stackentry: trace::StackEntryId,
) -> Result<(), Box<dyn Error>> {
    let mut total_blocks = 0;
    if let Some(summary) = transaction.summary(stackentry) {
        total_blocks = summary.alloc_count;
    }
    if total_blocks == 0 {
        return Ok(());
    }

    let mut ancestor = transaction
        .stackentry(stackentry)
        .ok_or("missing stackentry")?
        .next;
    while ancestor.is_some() {
        let ancestor_id = ancestor.unwrap();
        transaction.increment_descendents(ancestor_id)?;
        ancestor = transaction
            .stackentry(ancestor_id)
            .ok_or("missing stackentry")?
            .next;
    }

    Ok(())
}

// Print to stdout an indication of how complete the summarization process is.
fn summary_progress(
    start: time::Instant,
    now: time::Instant,
    noun: &str,
    current_id: u64,
    max_id: u64,
) {
    print!(
        "{}/{} {} processed ({:.2?})        \r",
        current_id,
        max_id,
        noun,
        now - start
    );
    io::stdout().flush().unwrap();
}

// Process a complete trace.  For each stack entry, generate a summary of the
// allocations made by each of its descendents.  Also, count the total number
// of descendents for each stack entry.
pub fn summarize_allocations(
    trace: &mut trace::Trace,
    show_progress: bool,
) -> Result<(), Box<dyn Error>> {
    let mut start_time = time::Instant::now();
    let mut last_time = start_time - time::Duration::new(1, 0);

    let max_event_id = trace.max_event_id()?;
    let max_stackentry_id = trace.max_stackentry_id()?;
    {
        let mut transaction = trace::Transaction::new(&trace)?;

        // Go through all events, adding allocations and frees to the summary.
        for event_id in 1..=max_event_id {
            if show_progress {
                let now = time::Instant::now();
                if now - last_time > time::Duration::from_millis(100) {
                    summary_progress(start_time, now, "events", event_id, max_event_id);
                    last_time = now;
                }
            }

            if let Some(event) = transaction.event(event_id) {
                let result = if event.allocation {
                    process_alloc(&mut transaction, &event)
                } else {
                    process_free(&mut transaction, &event)
                };
                match result {
                    Err(error) => eprintln!("Error processing event: {:?}", error),
                    Ok(_) => (),
                }
            }
        }

        if show_progress {
            let now = time::Instant::now();
            summary_progress(start_time, now, "events", max_event_id, max_event_id);
            println!("");
            start_time = now;
        }

        // Go through all stackentries, incrementing the descendent count of
        // their ancestors for each.
        for stackentry_id in 1..=max_stackentry_id {
            if show_progress {
                let now = time::Instant::now();
                if now - last_time > time::Duration::from_millis(100) {
                    summary_progress(start_time, now, "frames", stackentry_id, max_stackentry_id);
                    last_time = now;
                }
            }

            increment_descendent_counts(&mut transaction, stackentry_id)?;
        }

        transaction.commit()?;
    }

    if show_progress {
        let end_time = time::Instant::now();
        summary_progress(
            start_time,
            end_time,
            "frames",
            max_stackentry_id,
            max_stackentry_id,
        );
        println!("");
    }

    Ok(())
}
