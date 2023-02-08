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
use crate::context;
use crate::ptrace;
use crate::record::EventType;
use crate::unwind;
use libc;
use std::error::Error;

// Collect the current stack for a stopped thread.
fn collect_stack(
    context: &mut context::TraceContext,
    pid: u32,
) -> Result<Vec<unwind::StackEntry>, Box<dyn Error>> {
    context.ensure_thread_context(pid)?;
    let thread_context = context.get_thread_context(pid)?;

    unwind::collect_stack(
        &context.process_map,
        &context.symbol_index,
        &context.unwind_address_space,
        &thread_context.unwind_accessors,
    )
}

// Hook for mmap, which will resolve loose breakpoint bindings when a new
// binary is mapped into the traced process.
fn on_mmap(
    context: &mut context::TraceContext,
    pid: u32,
    complete: bool,
) -> Result<(), Box<dyn Error>> {
    if complete {
        context.update_process_map(pid)?;
    }

    Ok(())
}

// Hook for malloc, which will track the size of the allocation requested and
// set a breakpoint at the return address fo malloc completion.
fn on_malloc(context: &mut context::TraceContext, pid: u32) -> Result<(), Box<dyn Error>> {
    let regs = ptrace::getregs(pid)?;
    let size = regs.rdi;

    let stack = collect_stack(context, pid)?;
    if stack.len() >= 2 {
        context
            .breakpoint_set
            .add_one_shot_breakpoint(pid, stack[1].address, on_malloc_return)?;

        context
            .transaction
            .start_event(pid, EventType::Alloc(size), stack);
    }

    Ok(())
}

// Breakpoint callback for malloc completion.  Get the address of the
// allocation and finish recording the event.
fn on_malloc_return(context: &mut context::TraceContext, pid: u32) -> Result<(), Box<dyn Error>> {
    let regs = ptrace::getregs(pid)?;
    let address = regs.rax;

    context.transaction.complete_event(pid, address)?;

    Ok(())
}

// Handle calloc similarly to malloc, but do the size calculation using
// the size and count parameters.
fn on_calloc(context: &mut context::TraceContext, pid: u32) -> Result<(), Box<dyn Error>> {
    let regs = ptrace::getregs(pid)?;
    let count = regs.rdi;
    let size = regs.rsi;

    let stack = collect_stack(context, pid)?;
    if stack.len() >= 2 {
        context
            .breakpoint_set
            .add_one_shot_breakpoint(pid, stack[1].address, on_malloc_return)?;

        context
            .transaction
            .start_event(pid, EventType::Alloc(count * size), stack);
    }

    Ok(())
}

// Hook for realloc, which can be handled as malloc, but we will record as
// EventType::Realloc so that trace will record a free on the previous
// allocation if the reallocation is successful.
fn on_realloc(context: &mut context::TraceContext, pid: u32) -> Result<(), Box<dyn Error>> {
    let regs = ptrace::getregs(pid)?;
    let address = regs.rdi;
    let size = regs.rsi;

    let stack = collect_stack(context, pid)?;
    if stack.len() >= 2 {
        context
            .breakpoint_set
            .add_one_shot_breakpoint(pid, stack[1].address, on_malloc_return)?;

        context
            .transaction
            .start_event(pid, EventType::Realloc(address, size), stack);
    }

    Ok(())
}

// Hook for free.  No breakpoint on the return address this time, since we
// assume free will always succeed.
fn on_free(context: &mut context::TraceContext, pid: u32) -> Result<(), Box<dyn Error>> {
    let regs = ptrace::getregs(pid)?;
    let address = regs.rdi;
    let stack = collect_stack(context, pid)?;

    context.transaction.start_event(pid, EventType::Free, stack);
    context.transaction.complete_event(pid, address)?;

    Ok(())
}

// Add breakpoints for the standard allocation routines.
pub fn add_hooks(breakpoint_set: &mut breakpoint::BreakpointSet) -> Result<(), Box<dyn Error>> {
    breakpoint_set.add_syscall_intercept(libc::SYS_mmap, on_mmap);

    breakpoint_set.breakpoint_on("malloc", on_malloc);
    breakpoint_set.breakpoint_on("calloc", on_calloc);
    breakpoint_set.breakpoint_on("realloc", on_realloc);
    breakpoint_set.breakpoint_on("free", on_free);

    Ok(())
}
