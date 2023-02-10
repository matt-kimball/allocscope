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

use crate::breakpoint;
use crate::process_map;
use crate::record;
use crate::symbol_index;
use crate::unwind;
use std::collections::HashMap;
use std::error::Error;

// Context relevant to a single thread in the traced process.
pub struct TraceThreadContext {
    // true if the thread is currently in a system call.
    pub in_syscall: bool,

    // ptrace accessors used by libunwind to access the thread.
    pub unwind_context: unwind::UPTContext,
}

// Context relevant to the traced process.
pub struct TraceContext<'trace_lifetime> {
    // process-ID for the main thread of the process.
    pub pid: u32,

    // The set of active breakpoints in the process.
    pub breakpoint_set: breakpoint::BreakpointSet,

    // The SQL transaction used for recording trace data.
    pub transaction: record::Transaction<'trace_lifetime>,

    // A representation of the binaries mmap-ed into the process's
    // address space.
    pub process_map: process_map::ProcessMap,

    // Bookkeeping for the symbols from the binaries mmap-ed into the
    // process's address space.
    pub symbol_index: symbol_index::SymbolIndex,

    // Address space structure used by libunwind.
    pub unwind_address_space: unwind::AddressSpace,

    // Context for individual threads of the process.
    pub thread_context: HashMap<u32, TraceThreadContext>,
}

impl<'trace_lifetime> TraceContext<'trace_lifetime> {
    // Construct the context for tracing a new process.
    pub fn new(
        pid: u32,
        breakpoint_set: breakpoint::BreakpointSet,
        transaction: record::Transaction,
    ) -> Result<TraceContext, Box<dyn Error>> {
        Ok(TraceContext {
            pid,
            breakpoint_set,
            transaction,
            process_map: process_map::ProcessMap::new(pid)?,
            symbol_index: symbol_index::SymbolIndex::new(),
            unwind_address_space: unwind::AddressSpace::new_upt()?,
            thread_context: HashMap::new(),
        })
    }

    // Ensure that a context has been created for a given thread, creating
    // a new one if it doesn't already exist.
    pub fn ensure_thread_context(&mut self, pid: u32) -> Result<(), Box<dyn Error>> {
        if !self.thread_context.contains_key(&pid) {
            self.thread_context.insert(
                pid,
                TraceThreadContext {
                    in_syscall: false,
                    unwind_context: unwind::UPTContext::new(pid as i32)?,
                },
            );
        }

        Ok(())
    }

    // Get the mutable context for a particular thread.
    pub fn get_thread_context_mut(
        &mut self,
        pid: u32,
    ) -> Result<&mut TraceThreadContext, Box<dyn Error>> {
        self.thread_context
            .get_mut(&pid)
            .ok_or("missing thread context".into())
    }

    // Get a non-mutable context reference for a particular thread.
    pub fn get_thread_context(&self, pid: u32) -> Result<&TraceThreadContext, Box<dyn Error>> {
        self.thread_context
            .get(&pid)
            .ok_or("missing thread context".into())
    }

    // The memory map of the process we are tracing has changed, so update
    // the process map with all current memory mappings and reindex the
    // the symbols of the process as new code may have been mapped in.
    pub fn update_process_map(&mut self, pid: u32) -> Result<(), Box<dyn Error>> {
        self.process_map = process_map::ProcessMap::new(pid)?;
        self.symbol_index = symbol_index::SymbolIndex::new();
        self.symbol_index.add_symbols(&self.process_map);
        self.breakpoint_set
            .resolve_breakpoints(pid, &self.symbol_index)?;

        Ok(())
    }
}
