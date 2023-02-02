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
use crate::hooks;
use crate::ptrace;
use crate::record;
use std::error::Error;

// A breakpoint has been hit on one of our traced threads.  Now what?
// Determine what to do by checking for breakpoints and system call callbacks.
fn on_breakpoint(pid: u32, context: &mut context::TraceContext) -> Result<(), Box<dyn Error>> {
    context.ensure_thread_context(pid)?;
    let mut regs = ptrace::getregs(pid)?;

    let address = regs.rip - 1;
    let mut callback: Option<breakpoint::BreakpointCallback> = None;
    let mut intercept: Option<breakpoint::SyscallCallback> = None;
    let mut one_shot = false;

    match context.breakpoint_set.breakpoints.get(&address) {
        Some(breakpoint) => {
            // Move instruction pointer back one byte, because we will be
            // restoring the original instruction and stepping through.
            regs.rip = address;
            ptrace::setregs(pid, &regs)?;

            // If an event is already in progress, avoid invoking the callback
            // because some implementations of allocators may nest calls to
            // allocation functions.
            if breakpoint.persist && !context.transaction.is_event_in_progress(pid) {
                callback = Some(breakpoint.callback);
            }

            // If it is a one-shot breakpoint relevant to this thread, we
            // will always execute the callback.
            if breakpoint.one_shot_threads.contains(&pid) {
                callback = Some(breakpoint.callback);
                one_shot = true;
            }
        }

        None => {
            let insn = ptrace::peekbyte(pid, regs.rip - 2);
            let insn2 = ptrace::peekbyte(pid, regs.rip - 1);

            // Check for x86_64 'syscall' instruction (0F 05) to determine
            // whether our thread is stopped at a system call.
            if insn == 0x0F && insn2 == 0x05 {
                let syscall_id = regs.orig_rax as i64;
                match context.breakpoint_set.syscall_intercepts.get(&syscall_id) {
                    Some(callback) => {
                        intercept = Some(*callback);
                    }
                    None => (),
                }
            }
        }
    }

    // Dispatch to a breakpoint callback, if appropriate.
    if let Some(func) = callback {
        match func(context, pid) {
            Ok(()) => (),
            Err(err) => eprintln!("Error on breakpoint: {:?}", err),
        }
    }

    // Dispatch to a system-call intercept, if appropriate.
    if let Some(func) = intercept {
        let in_syscall = context.get_thread_context(pid)?.in_syscall;
        match func(context, pid, in_syscall) {
            Ok(()) => (),
            Err(err) => eprintln!("Error on syscall: {:?}", err),
        }

        let thread_context = context.get_thread_context_mut(pid)?;
        thread_context.in_syscall = !thread_context.in_syscall;
    }

    // Step through the breakpoint, if there is one at our stopped address.
    if let Some(breakpoint) = context.breakpoint_set.breakpoints.get(&address) {
        // Ensure other threads are stopped while we remove the breakpoint
        // and single-step, to avoid missing events where the other
        // threads hit this breakpoint while we are single stepping.
        ptrace::cont(pid, libc::SIGSTOP as u8)?;
        wait_for_signal(pid, libc::SIGSTOP)?;

        // Step through the breakpoint instruction.
        breakpoint.step_through(pid)?;
    }

    // If we hit a one-shot breakpoint, we should remove it now that it has
    // been active.
    if one_shot {
        context
            .breakpoint_set
            .remove_one_shot_breakpoint(pid, address)?;
    }

    Ok(())
}

// Wait for a particular signal to be received by one of our traced threads.
pub fn wait_for_signal(pid: u32, wait_signal: i32) -> Result<(), Box<dyn Error>> {
    loop {
        let (_, status) = ptrace::waitpid(pid as i32, true)?;
        match status {
            ptrace::WaitPidResult::Stopped(signal) => {
                if signal as i32 == wait_signal {
                    break;
                } else {
                    // If we received some other signal, dispatch it to
                    // the traced thread.
                    ptrace::cont(pid, signal)?
                }
            }
            _ => Err("program termination while waiting for signal")?,
        }
    }

    Ok(())
}

// Execute the main loop of the trace.  This assumes we have already attached
// to a process to trace, and have a TraceContext relevant to the process.
fn trace_loop(context: &mut context::TraceContext, pid: u32) -> Result<(), Box<dyn Error>> {
    loop {
        let (status_pid, status) = ptrace::waitpid(-1, true)?;
        match status {
            // One of our traced threads has stopped.
            ptrace::WaitPidResult::Stopped(signal) => match signal as i32 {
                // SIGTRAP indicates a traced thread hit a breakpoint.
                libc::SIGTRAP => {
                    on_breakpoint(status_pid, context)?;

                    // Swallow the SIGTRAP signal, since we handled it.
                    ptrace::syscall(status_pid, 0)?;
                }

                // Pass along other signals to the traced thread.
                _ => ptrace::syscall(status_pid, signal)?,
            },

            // A traced thread has spawned a new thread via clone.
            ptrace::WaitPidResult::EventClone => {
                let new_thread = ptrace::geteventmsg(status_pid)?;

                wait_for_signal(new_thread, libc::SIGSTOP)?;

                // Resume execution of both the spawning thread and the new
                // thread.
                ptrace::syscall(new_thread, 0)?;
                ptrace::syscall(status_pid, 0)?;
            }

            // Otherwise, either the traced process has exited, or we
            // encountered an unexpected waitpid result.  Either way, time
            // to stop the trace.
            _ => {
                match status {
                    ptrace::WaitPidResult::Exited(_) => (),
                    _ => eprintln!("Unknown waitpid result {}: {:?}", status_pid, status),
                }

                if status_pid == pid {
                    return Ok(());
                }
            }
        }
    }
}

// Detatch from our traced process, removing all breakpoints we set, and
// resuming execution of the original process.
fn detach_from_tracee(context: &mut context::TraceContext) -> Result<(), Box<dyn Error>> {
    let (status_pid, status) = ptrace::waitpid(-1, false)?;
    let stop_signal = match status {
        ptrace::WaitPidResult::Stopped(signal) => signal,
        _ => 0,
    };
    context.breakpoint_set.clear_breakpoints(status_pid)?;
    ptrace::detach(status_pid, stop_signal)?;
    ptrace::kill(status_pid, libc::SIGCONT)?;

    Ok(())
}

// Start a new trace of a given process-id.  This path is common between
// both processes we spawn and pre-existing processes to which we are
// attaching.
fn trace_attached_pid(record: record::TraceRecord, pid: u32) -> Result<(), Box<dyn Error>> {
    let mut breakpoint_set = breakpoint::BreakpointSet::new();
    hooks::add_hooks(&mut breakpoint_set)?;
    breakpoint_set.resolve_breakpoints(pid)?;
    ptrace::setoptions(pid, libc::PTRACE_O_TRACECLONE)?;

    // Now that we have set breakpoints, resume execution.
    ptrace::syscall(pid, 0)?;

    let transaction = record::Transaction::new(&record)?;
    let mut context = context::TraceContext::new(pid, breakpoint_set, transaction)?;

    ptrace::block_term_signals()?;
    match trace_loop(&mut context, pid) {
        Err(err) => {
            // If we have received SIGTERM or SIGINT while tracing, cleanly
            // detach and complete the trace file.
            if err.is::<ptrace::SignaledError>() {
                println!("Trace terminated by signal");
                detach_from_tracee(&mut context)?;

                ()
            } else {
                Err(err)?
            }
        }
        Ok(()) => (),
    }
    context.transaction.commit()?;

    Ok(())
}

// Attach to an existing process and trace it.
pub fn trace_pid(record: record::TraceRecord, pid: u32) -> Result<(), Box<dyn Error>> {
    ptrace::attach(pid)?;
    wait_for_signal(pid, libc::SIGSTOP)?;

    return trace_attached_pid(record, pid);
}

// Spawn a new process from a given commandline and trace it.
pub fn trace_command(
    record: record::TraceRecord,
    command: &Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let pid = ptrace::attach_to_child_exec(&command)?;
    wait_for_signal(pid, libc::SIGTRAP)?;

    return trace_attached_pid(record, pid);
}
