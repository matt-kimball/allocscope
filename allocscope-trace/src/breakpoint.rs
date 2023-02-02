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

use crate::context;
use crate::process_map;
use crate::ptrace;
use crate::symbol_index;
use crate::trace;
use std::collections::{HashMap, HashSet};
use std::error::Error;

// A callback invoked with a breakpoint it triggered in a traced process.
pub type BreakpointCallback =
    fn(context: &mut context::TraceContext, pid: u32) -> Result<(), Box<dyn Error>>;

// A callback invoked when a system call is made by a traced process.
//
// 'complete' will be false as the system call is entered, and true as it
// exits.
pub type SyscallCallback =
    fn(context: &mut context::TraceContext, pid: u32, complete: bool) -> Result<(), Box<dyn Error>>;

// Tracking data for a breakpoint.
pub struct Breakpoint {
    // The instruction address at which the breakpoint was inserted.
    pub address: u64,

    // The original instructions at the 8-byte aligned address where
    // the breakpoint was insertered.
    pub original_instruction: u64,

    // The callback to invoke when the breakpoint is hit.
    pub callback: BreakpointCallback,

    // true if the breakpoint should remain after being encountered.
    // false for one shot breakpoints.
    pub persist: bool,

    // A set of thread on which the callback should be invoked when
    // the breakpoint is encountered.  Used by one shot breakpoints.
    pub one_shot_threads: HashSet<u32>,
}

// The binding between an unresolved symbol name (function name) and
// the callback to invoke when the breakpoint is encountered.
pub struct BreakpointLooseBinding {
    // Name of function at which set set the breakpoint.
    pub function_name: String,

    // The callback to invoke.
    pub callback: BreakpointCallback,
}

// The set of all breakpoints relevant to a traced process.
pub struct BreakpointSet {
    // The bindings between function names and callbacks.
    pub bindings: Vec<BreakpointLooseBinding>,

    // The breakpoint instructions which have been inserted into the process.
    pub breakpoints: HashMap<u64, Breakpoint>,

    // Intercepted system calls for the process.
    pub syscall_intercepts: HashMap<i64, SyscallCallback>,
}

// Insert a breakpoint in the address space of the traced process.
fn insert_breakpoint_instruction(pid: u32, address: u64) -> Result<(), Box<dyn Error>> {
    // The peektext / poketext are 8-byte aligned, but x86_64 instructions are
    // not, so we need to shift the appropriate byte.
    let shift = (address & 7) * 8;
    let code = ptrace::peektext(pid, address & !7);

    // The x86_64 instruction 'int3' is encoded as 0xCC.
    let instruction = (0xCC << shift) | (code & !(0xFF << shift));

    ptrace::poketext(pid, address & !7, instruction)?;

    Ok(())
}

// Remove a previously inserted breakpoint, restoring the original instruction.
fn remove_breakpoint_instruction(
    pid: u32,
    address: u64,
    original_instruction: u64,
) -> Result<(), Box<dyn Error>> {
    // The peektext / poketext are 8-byte aligned, but x86_64 instructions are
    // not, so we need to shift the appropriate byte.
    let shift = (address & 7) * 8;
    let code = ptrace::peektext(pid, address & !7);

    // We want to restore only one byte, rather than the entire 8-byte word,
    // because there could be other inserted breakpoints within the same
    // word which we don't want to disrupt.
    let instruction = (original_instruction & (0xFF << shift)) | (code & !(0xFF << shift));

    ptrace::poketext(pid, address & !7, instruction)?;

    Ok(())
}

// Insert a breakpoint into a traced process, taking care to handle the case
// where a breakpoint has already been inserted at the same address, and
// update the bookkeeping for one-shot breakpoints.
fn add_breakpoint(
    breakpoints: &mut HashMap<u64, Breakpoint>,
    pid: u32,
    address: u64,
    callback: BreakpointCallback,
    persist: bool,
) -> Result<(), Box<dyn Error>> {
    // It may be that another thread wants a one-shot breakpoint at the same
    // address.  In such a case, avoid a double insert so that we don't
    // read the previously inserted breakpoint as the "original" instruction.
    if !breakpoints.contains_key(&address) {
        let original_instruction = ptrace::peektext(pid, address & !7);
        insert_breakpoint_instruction(pid, address)?;

        let breakpoint = Breakpoint {
            address,
            original_instruction,
            callback,
            persist,
            one_shot_threads: HashSet::new(),
        };
        breakpoints.insert(address, breakpoint);
    }

    // Regardless of whether we modified instructions above, track this
    // thread as interested if this is a one shot breakpoint.
    let breakpoint = breakpoints.get_mut(&address).ok_or("breakpoint missing")?;
    if !persist {
        breakpoint.one_shot_threads.insert(pid);
    }

    Ok(())
}

impl Breakpoint {
    // Step through a breakpoint by restoring the instruction which was
    // replaced when the breakpoint was set, stepping through that one
    // instruction, and then putting the breakpoint back.
    pub fn step_through(&self, pid: u32) -> Result<(), Box<dyn Error>> {
        self.remove_breakpoint_instruction(pid)?;
        ptrace::singlestep(pid)?;
        trace::wait_for_signal(pid, libc::SIGTRAP)?;
        insert_breakpoint_instruction(pid, self.address)?;

        Ok(())
    }

    // Remove the breakpoint by restoring the original instruction.
    fn remove_breakpoint_instruction(&self, pid: u32) -> Result<(), Box<dyn Error>> {
        remove_breakpoint_instruction(pid, self.address, self.original_instruction)
    }
}

impl BreakpointSet {
    // Create a new empty set of breakpoints for a traced process.
    pub fn new() -> BreakpointSet {
        BreakpointSet {
            bindings: Vec::new(),
            breakpoints: HashMap::new(),
            syscall_intercepts: HashMap::new(),
        }
    }

    // Add a one shot breakpoint at a specific address.  This is used
    // following a stack trace to breakpoint at the return of a function.
    pub fn add_one_shot_breakpoint(
        &mut self,
        pid: u32,
        address: u64,
        callback: BreakpointCallback,
    ) -> Result<(), Box<dyn Error>> {
        add_breakpoint(&mut self.breakpoints, pid, address, callback, false)
    }

    // Disable a one shot breakpoint for a particular thread.
    pub fn remove_one_shot_breakpoint(
        &mut self,
        pid: u32,
        address: u64,
    ) -> Result<(), Box<dyn Error>> {
        let breakpoint = self
            .breakpoints
            .get_mut(&address)
            .ok_or("breakpoint missing")?;
        breakpoint.one_shot_threads.remove(&pid);

        Ok(())
    }

    // Break at the entry point of a particular function name.
    // This only creates a loose binding (binding by function name) because
    // the relevant code may not be mapped into the process yet.
    pub fn breakpoint_on(&mut self, function_name: &str, callback: BreakpointCallback) {
        self.bindings.push(BreakpointLooseBinding {
            function_name: function_name.to_string(),
            callback: callback,
        });
    }

    // Add a callback for a particular system call.
    pub fn add_syscall_intercept(&mut self, syscall_id: i64, callback: SyscallCallback) {
        self.syscall_intercepts.insert(syscall_id, callback);
    }

    // Rebind all previously bound breakpoints.  Used when new symbols may
    // have been resolved.
    fn rebind_breakpoints(&mut self, pid: u32) -> Result<(), Box<dyn Error>> {
        for breakpoint in self.breakpoints.values() {
            insert_breakpoint_instruction(pid, breakpoint.address)?;
        }

        Ok(())
    }

    // Resolve all loosely bound breakpoint using the current process map of
    // the traced process.
    pub fn resolve_breakpoints(&mut self, pid: u32) -> Result<(), Box<dyn Error>> {
        let process_map = process_map::ProcessMap::new(pid)?;
        let mut symbol_index = symbol_index::SymbolIndex::new();
        symbol_index.add_symbols(&process_map);

        for binding in self.bindings.iter() {
            match symbol_index.symbols.get(&binding.function_name) {
                Some(entry) => {
                    // For each address of the function, set a breakpoint.
                    // Multiple addresses might be necessary, because there
                    // might be multiple linked copies of a function with the
                    // same name.  (Consider multiple linked copies of libc
                    // in the same process.)
                    for address in &entry.addresses {
                        if !self.breakpoints.contains_key(address) {
                            add_breakpoint(
                                &mut self.breakpoints,
                                pid,
                                *address,
                                binding.callback,
                                true,
                            )?;
                        }
                    }
                }
                None => (),
            }
        }

        // XXX: This will cause damage if a shared library is unmapped but we
        // still have the breakpoint.
        self.rebind_breakpoints(pid)?;

        Ok(())
    }

    // Remove all previously inserted breakpoints from the process.  Used
    // when deatching from a process to leave it in a runnable state when
    // not being traced.
    pub fn clear_breakpoints(&mut self, pid: u32) -> Result<(), Box<dyn Error>> {
        for breakpoint in self.breakpoints.values() {
            breakpoint.remove_breakpoint_instruction(pid)?;
        }

        Ok(())
    }
}
