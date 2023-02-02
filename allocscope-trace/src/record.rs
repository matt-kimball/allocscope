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

use crate::unwind;
use rusqlite;
use std::collections::HashMap;
use std::error::Error;
use std::fs;

// The event type of an allocation event currently in progress on a traced
// thread.
#[derive(PartialEq)]
pub enum EventType {
    // An allocation, with an included size.
    Alloc(u64),

    // A reallocation, with the address of the previous allocation and
    // a new size.
    Realloc(u64, u64),

    // A free of a previous allocation.
    Free,
}

// An in-progress allocation event associate with a particular thread we
// are tracing.
struct RecordInProgress {
    // The allocation event.
    allocation: EventType,

    // The callstack from the start of the event.
    callstack: Vec<unwind::StackEntry>,
}

// A record of a trace in progress.
pub struct TraceRecord {
    // The SQLite connection to the database.
    connection: rusqlite::Connection,
}

// A SQLite transaction currently in progress, used to record trace data.
pub struct Transaction<'trace_lifetime> {
    // The trace which owns this transaction.
    pub record: &'trace_lifetime TraceRecord,

    // A map from PID of traced threads to any allocations currently
    // in progress.
    record_in_progress: HashMap<u32, RecordInProgress>,

    // Prepared SQL for inserting a new location.
    location_insert_statement: rusqlite::Statement<'trace_lifetime>,

    // Prepared SQL For selecting a location.
    location_select_statement: rusqlite::Statement<'trace_lifetime>,

    // Prepared SQL for inserting a callstack frame with a parent frame.
    callstack_insert_with_next: rusqlite::Statement<'trace_lifetime>,

    // Prepared SQL for inserting a callstack frame with no parent.
    callstack_insert_no_next: rusqlite::Statement<'trace_lifetime>,

    // Prepared SQL for selecting a callstack with a specific location and
    // parent frame.
    callstack_select_with_next: rusqlite::Statement<'trace_lifetime>,

    // Prepared SQL for selecting a callstack with a specific location and
    // no parent frame.
    callstack_select_no_next: rusqlite::Statement<'trace_lifetime>,

    // Prepared SQL for inserting a new event.
    insert_event_statement: rusqlite::Statement<'trace_lifetime>,
}

impl<'trace_lifetime> Transaction<'trace_lifetime> {
    // Start a new transaction, preparing SQL statements which we are
    // likely to need.
    pub fn new(
        record: &'trace_lifetime TraceRecord,
    ) -> Result<Transaction<'trace_lifetime>, Box<dyn Error>> {
        record.connection.execute("BEGIN TRANSACTION", [])?;

        Ok(Transaction {
            record,
            record_in_progress: HashMap::new(),

            location_insert_statement: record.connection.prepare(
                "INSERT INTO location (address, function, offset)
                    SELECT ?, ?, ?
                    WHERE NOT EXISTS (
                        SELECT TRUE FROM location WHERE
                            address = ? AND function = ? AND offset = ?
                    )",
            )?,
            location_select_statement: record.connection.prepare(
                "SELECT id FROM location WHERE
                    address = ? AND function = ? AND offset = ?",
            )?,
            callstack_insert_with_next: record.connection.prepare(
                "INSERT INTO stackentry (location, next)
                SELECT ?, ?
                WHERE NOT EXISTS (
                SELECT TRUE FROM stackentry WHERE
                    location = ? AND next = ?
                )",
            )?,
            callstack_select_with_next: record.connection.prepare(
                "SELECT id FROM stackentry WHERE
                location = ? AND next = ?",
            )?,
            callstack_insert_no_next: record.connection.prepare(
                "INSERT INTO stackentry (location, next)
                SELECT ?, NULL
                WHERE NOT EXISTS (
                SELECT TRUE FROM stackentry WHERE
                    location = ? AND next IS NULL
                )",
            )?,
            callstack_select_no_next: record.connection.prepare(
                "SELECT id FROM stackentry WHERE
                location = ? AND next IS NULL",
            )?,
            insert_event_statement: record.connection.prepare(
                "INSERT INTO event (time, allocation, address, size, callstack)
                    VALUES (datetime('now'), ?, ?, ?, ?)",
            )?,
        })
    }

    // Commit changes in the current transaction to the database.
    pub fn commit(&mut self) -> Result<(), Box<dyn Error>> {
        self.record.connection.execute("COMMIT", []).unwrap();

        Ok(())
    }

    // Insert code locations referenced by a callstack.
    fn insert_locations(
        &mut self,
        callstack: &Vec<unwind::StackEntry>,
    ) -> Result<Vec<u64>, Box<dyn Error>> {
        let mut locations: Vec<u64> = Vec::new();

        for entry in callstack {
            self.location_insert_statement.execute(rusqlite::params![
                entry.address,
                entry.name,
                entry.offset,
                entry.address,
                entry.name,
                entry.offset,
            ])?;

            let mut rows = self.location_select_statement.query(rusqlite::params![
                entry.address,
                entry.name,
                entry.offset
            ])?;
            let row = rows.next()?.ok_or("failure selecting inserted location")?;
            locations.push(row.get(0)?);
        }

        Ok(locations)
    }

    // Insert a callstack which references a list of code locations previously
    // inserted in the location table.
    fn insert_callstack(&mut self, locations: &Vec<u64>) -> Result<Option<u64>, Box<dyn Error>> {
        let mut last_entry_id: Option<u64> = None;

        // Insert in reverse order because we are starting with the root and
        // including an id of the parent in each child entry.
        for ix in (0..locations.len()).rev() {
            let location = locations[ix];

            match last_entry_id {
                Some(last_entry) => {
                    self.callstack_insert_with_next.execute(rusqlite::params![
                        location, last_entry, location, last_entry
                    ])?;

                    let mut rows = self
                        .callstack_select_with_next
                        .query(rusqlite::params![location, last_entry])?;
                    let row = rows.next()?.ok_or("failure selecting inserted location")?;
                    last_entry_id = row.get(0).ok();
                }
                None => {
                    self.callstack_insert_no_next
                        .execute(rusqlite::params![location, location])?;

                    let mut rows = self
                        .callstack_select_no_next
                        .query(rusqlite::params![location])?;
                    let row = rows.next()?.ok_or("failure selecting inserted location")?;
                    last_entry_id = row.get(0).ok();
                }
            }
        }

        Ok(last_entry_id)
    }

    // Insert an entry into the allocation event table.
    fn insert_event(
        &mut self,
        allocation: bool,
        address: u64,
        size: Option<u64>,
        callstack_id: Option<u64>,
    ) -> Result<(), Box<dyn Error>> {
        self.insert_event_statement.execute(rusqlite::params![
            allocation,
            address,
            match size {
                Some(_) => size.as_ref().unwrap() as &dyn rusqlite::ToSql,
                None => &rusqlite::types::Null as &dyn rusqlite::ToSql,
            },
            match callstack_id {
                Some(_) => callstack_id.as_ref().unwrap() as &dyn rusqlite::ToSql,
                None => &rusqlite::types::Null as &dyn rusqlite::ToSql,
            },
        ])?;

        Ok(())
    }

    // Return true if a given thread currently has an event in progress.
    pub fn is_event_in_progress(&self, pid: u32) -> bool {
        self.record_in_progress.contains_key(&pid)
    }

    // Start recording an allocation event associated with a particular
    // thread.
    pub fn start_event(
        &mut self,
        pid: u32,
        allocation: EventType,
        callstack: Vec<unwind::StackEntry>,
    ) {
        self.record_in_progress.insert(
            pid,
            RecordInProgress {
                allocation,
                callstack,
            },
        );
    }

    // Complete a previously started event with an address for the allocation.
    pub fn complete_event(&mut self, pid: u32, address: u64) -> Result<(), Box<dyn Error>> {
        let record_in_progress = self
            .record_in_progress
            .remove(&pid)
            .ok_or("Completing event with none in-progress")?;

        let locations = self.insert_locations(&record_in_progress.callstack)?;
        let callstack_id = self.insert_callstack(&locations)?;

        match record_in_progress.allocation {
            EventType::Alloc(size) => {
                if address != 0 {
                    self.insert_event(true, address, Some(size), callstack_id)?
                }
            }
            EventType::Free => {
                if address != 0 {
                    self.insert_event(false, address, None, callstack_id)?
                }
            }
            EventType::Realloc(original_address, size) => {
                if original_address != 0 && (address != 0 || size == 0) {
                    self.insert_event(false, original_address, None, callstack_id)?;
                }
                if address != 0 {
                    self.insert_event(true, address, Some(size), callstack_id)?;
                }
            }
        }

        Ok(())
    }
}

impl TraceRecord {
    // Start a new trace file with a given filename.
    pub fn new(filename: &str) -> Result<TraceRecord, Box<dyn Error>> {
        // First remove any existing file, so we can replace it.
        _ = fs::remove_file(filename);

        println!("Recording trace to {}", filename);

        let connection = rusqlite::Connection::open(filename)?;

        connection.execute(
            "CREATE TABLE IF NOT EXISTS trace (
                version TEXT NOT NULL,
                time TEXT NOT NULL
            )",
            [],
        )?;

        connection.execute(
            "CREATE TABLE IF NOT EXISTS event (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                time TEXT NOT NULL,
                allocation BOOLEAN NOT NULL,
                address INTEGER NOT NULL,
                size INTEGER,
                callstack INTEGER
            )",
            [],
        )?;

        connection.execute(
            "CREATE TABLE IF NOT EXISTS stackentry (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                location INTEGER NOT NULL,
                next INTEGER
            )",
            [],
        )?;

        connection.execute(
            "CREATE TABLE IF NOT EXISTS location (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                address INTEGER NOT NULL,
                function TEXT,
                offset INTEGER
            )",
            [],
        )?;

        connection.execute("CREATE INDEX location_address_ix ON location (address)", [])?;

        connection.execute(
            "CREATE INDEX stackentry_location_ix ON stackentry (location)",
            [],
        )?;
        connection.execute("CREATE INDEX stackentry_next_ix ON stackentry (next)", [])?;

        // Store the version of the program creating the trace for future
        // compatibility checks.
        let version = env!("CARGO_PKG_VERSION");
        connection.execute(
            "INSERT INTO trace (version, time)
                VALUES (?, datetime('now'))",
            rusqlite::params![version],
        )?;

        Ok(TraceRecord { connection })
    }
}
