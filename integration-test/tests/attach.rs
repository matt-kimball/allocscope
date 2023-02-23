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

use integration_test;
use libc;
use std::error::Error;
use std::process;

// Read pending output from the stdout of a child process.  Used to synchronize
// the state of the child process with the test case, because we know that the
// traced process will output a single line of text every iteration of its
// periodic allocation loop.
fn read_child_output(stdout: &mut impl std::io::Read) -> Result<String, Box<dyn Error>> {
    let mut buffer: [u8; 1024] = [0; 1024];
    stdout.read(&mut buffer)?;
    let line = std::str::from_utf8(&buffer)?;

    Ok(line.to_string())
}

// Spawn a process which will loop forever, doing an allocation one per second.
// After that process is started, attach allocscope-trace by PID to perform a
// trace.  After at least one full iteration of the tracee's allocation loop,
// interupt the trace and detach.
#[test]
fn test_attach() -> Result<(), Box<dyn Error>> {
    let binary_path = integration_test::compile_source("forever.c")?;
    let forever_process = process::Command::new(&binary_path)
        .stdout(process::Stdio::piped())
        .spawn()?;
    let forever_pid = forever_process.id();
    let mut forever_stdout = forever_process.stdout.ok_or("stdout not captured")?;

    // Wait for the tracee to start.
    read_child_output(&mut forever_stdout)?;

    let trace_path = format!("{}.atrace", binary_path);
    let mut trace_process = process::Command::new(std::env::var("TEST_ALLOCSCOPE_TRACE")?)
        .args(["-o", &trace_path, "-p", &format!("{}", forever_pid)])
        .spawn()?;
    let trace_pid = trace_process.id();

    // Consume any output generated while spawning trace.
    read_child_output(&mut forever_stdout)?;

    // Allow the trace time to attach.
    read_child_output(&mut forever_stdout)?;

    // Wait for the next iteration of the allocation loop to be traced.
    read_child_output(&mut forever_stdout)?;

    unsafe {
        libc::kill(trace_pid as i32, libc::SIGINT);
    }

    // Wait for the trace to finish.
    trace_process.wait()?;

    let view_result = integration_test::view_trace(&trace_path);
    std::fs::remove_file(&trace_path)?;
    let trace = view_result?;

    unsafe {
        libc::kill(forever_pid as i32, libc::SIGKILL);
    }
    std::fs::remove_file(&binary_path)?;

    let leaf_ix = integration_test::find_top_leaf_index(&trace).ok_or("no top leaf")?;
    let line = &trace[leaf_ix - 1];

    assert_eq!(line.bytes, "1024k");
    assert_eq!(line.leaks, "0");
    assert_eq!(line.tree.len(), 8);
    assert!(line.name.contains("step"));

    Ok(())
}
