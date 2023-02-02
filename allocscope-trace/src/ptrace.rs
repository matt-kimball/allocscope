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

use libc;
use std::error::Error;
use std::fmt;
use std::ptr;

// A custom error to propagate when the tracing process receives a signal
// to stop.  (SIGTERM, SIGINT)
#[derive(Debug)]
pub struct SignaledError;

// A result from the waitpid system call.
#[derive(Debug)]
pub enum WaitPidResult {
    // The process exited.  Included is the exit value.
    Exited(u8),

    // The process receive a signal.  Included is the signal value.
    Signaled(u8),

    // The process has been stopped.  Included is the signal value.
    Stopped(u8),

    // A clone event has occurred, spawning a new thread.
    EventClone,
}

impl Error for SignaledError {}

impl fmt::Display for SignaledError {
    // Formatted SignalledError.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "received terminating signal")
    }
}

// A string representing the current value of C's 'errno', for reporting
// errors from calls through libc.
fn errno_string() -> String {
    unsafe {
        let errno = *libc::__errno_location();
        std::ffi::CStr::from_ptr(libc::strerror(errno))
            .to_string_lossy()
            .into_owned()
    }
}

// Attach a trace to an existing process.
pub fn attach(pid: u32) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_ATTACH, pid, 0, 0) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Continue a ptraced process's execution.
pub fn cont(pid: u32, signal: u8) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_CONT, pid, 0, signal as libc::c_uint) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Detach from a process which is currently being traced.
pub fn detach(pid: u32, signal: u8) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_DETACH, pid, 0, signal as libc::c_uint) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Continue a ptraced process, but stop at the next system call.
pub fn syscall(pid: u32, signal: u8) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_SYSCALL, pid, 0, signal as libc::c_uint) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Get the CPU register contents of a current stopped ptraced process.
pub fn getregs(pid: u32) -> Result<libc::user_regs_struct, Box<dyn Error>> {
    unsafe {
        let mut regs = std::mem::MaybeUninit::<libc::user_regs_struct>::zeroed().assume_init();

        if libc::ptrace(libc::PTRACE_GETREGS, pid, 0, &mut regs) == -1 {
            Err(errno_string())?
        } else {
            Ok(regs)
        }
    }
}

// Set the CPU register contents of a current stopped ptraced process.
pub fn setregs(pid: u32, regs: &libc::user_regs_struct) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_SETREGS, pid, 0, regs) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Read an 8-byte word of code from a stopped ptraced process.
pub fn peektext(pid: u32, address: u64) -> u64 {
    unsafe { libc::ptrace(libc::PTRACE_PEEKTEXT, pid, address, 0) as u64 }
}

// Read an individual byte of code from a stopped ptraced process.
pub fn peekbyte(pid: u32, address: u64) -> u8 {
    ((peektext(pid, address & !7) >> ((address & 7) * 8)) & 0xFF) as u8
}

// Write an 8-byte word of code to a stopped ptraced process.
pub fn poketext(pid: u32, address: u64, instruction: u64) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_POKETEXT, pid, address, instruction) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Step through a single instruction of a stopped ptraced process.
pub fn singlestep(pid: u32) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_SINGLESTEP, pid, 0, 0) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Set ptrace options on a stopped process.
pub fn setoptions(pid: u32, options: i32) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::ptrace(libc::PTRACE_SETOPTIONS, pid, 0, options) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Get the ptrace event message for a stopped process.
// Can be used to get the PID of a newly spawned thread after a clone syscall.
pub fn geteventmsg(pid: u32) -> Result<u32, Box<dyn Error>> {
    let mut result: u32 = 0;

    unsafe {
        if libc::ptrace(libc::PTRACE_GETEVENTMSG, pid, 0, &mut result) == -1 {
            Err(errno_string())?
        } else {
            Ok(result)
        }
    }
}

// Send a signal to a thread.
pub fn kill(pid: u32, signal: i32) -> Result<(), Box<dyn Error>> {
    unsafe {
        if libc::kill(pid as i32, signal) == -1 {
            Err(errno_string())?
        } else {
            Ok(())
        }
    }
}

// Wait for an event from a child.  In our case, we will use it to wait for
// events in a ptraced process.
pub fn waitpid(
    pid: i32,
    check_pending_signals: bool,
) -> Result<(u32, WaitPidResult), Box<dyn Error>> {
    // Upon entry, we check for terminating signals and return SignaledError
    // in such a case, so we can then stop the trace.
    if check_pending_signals && is_term_signal_pending()? {
        return Err(SignaledError {})?;
    }

    unsafe {
        let mut status: i32 = 0;

        let result = libc::waitpid(pid, &mut status, 0);
        if result == -1 {
            Err(errno_string())?
        } else if status >> 16 == libc::PTRACE_EVENT_CLONE {
            Ok((result as u32, WaitPidResult::EventClone))
        } else {
            Ok(if libc::WIFEXITED(status) {
                (
                    result as u32,
                    WaitPidResult::Exited(libc::WEXITSTATUS(status) as u8),
                )
            } else if libc::WIFSIGNALED(status) {
                (
                    result as u32,
                    WaitPidResult::Signaled(libc::WTERMSIG(status) as u8),
                )
            } else if libc::WIFSTOPPED(status) {
                (
                    result as u32,
                    WaitPidResult::Stopped(libc::WSTOPSIG(status) as u8),
                )
            } else {
                Err("Unexpected waitpid result")?
            })
        }
    }
}

// Block signals which request termination of the process: SIGTERM, SIGINT.
// We will check upon entry to waitpid for pending signals, so we will still
// react appropriately.
pub fn block_term_signals() -> Result<(), Box<dyn Error>> {
    unsafe {
        let mut sigset = std::mem::MaybeUninit::<libc::sigset_t>::zeroed().assume_init();

        if libc::sigemptyset(&mut sigset) == -1 {
            Err(errno_string())?
        }
        if libc::sigaddset(&mut sigset, libc::SIGTERM) == -1 {
            Err(errno_string())?
        }
        if libc::sigaddset(&mut sigset, libc::SIGINT) == -1 {
            Err(errno_string())?
        }
        if libc::sigprocmask(libc::SIG_BLOCK, &mut sigset, ptr::null_mut()) == -1 {
            Err(errno_string())?
        }
    }

    Ok(())
}

// Returns true if a blocked termination signal is pending for the trace
// process, false otherwise.
pub fn is_term_signal_pending() -> Result<bool, Box<dyn Error>> {
    unsafe {
        let mut sigset = std::mem::MaybeUninit::<libc::sigset_t>::zeroed().assume_init();

        if libc::sigpending(&mut sigset) == -1 {
            Err(errno_string())?
        }

        Ok(libc::sigismember(&sigset, libc::SIGTERM) != 0
            || libc::sigismember(&sigset, libc::SIGINT) != 0)
    }
}

// fork off a new child and exec a given command.  This new process will
// be attached as a tracee prior to exec.
//
// Returns the pid of the new process.
pub fn attach_to_child_exec(command: &Vec<String>) -> Result<u32, Box<dyn Error>> {
    let mut cstrings: Vec<std::ffi::CString> = Vec::new();
    let mut args: Vec<*const libc::c_char> = Vec::new();
    for arg in command {
        let cstring = std::ffi::CString::new(arg.clone())?;
        args.push(cstring.as_ptr());
        cstrings.push(cstring);
    }
    args.push(ptr::null());

    let pid;
    unsafe {
        pid = libc::fork();
        if pid == 0 {
            libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0);
            libc::execvp(args[0], args.as_ptr());
            libc::exit(1);
        }
    }

    Ok(pid as u32)
}
