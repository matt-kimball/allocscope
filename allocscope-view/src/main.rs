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

mod commandline;
mod report;
mod rows;
mod summary;
mod trace;
mod ui;

use libc;
use std::error::Error;

// The main entry point for allocscope-view.
fn main() -> Result<(), Box<dyn Error>> {
    let args = commandline::CommandLineArguments::parse(&mut std::env::args())?;
    if args.report_version {
        commandline::report_version();
        return Ok(());
    }
    if args.show_help || args.atrace_filename.is_none() {
        commandline::show_help();
        return Ok(());
    }

    let is_stdout_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) != 0 };
    let report_mode = args.report_mode || !is_stdout_tty;

    let scratch_filename = format!("/tmp/trace-view-{}.scratch", std::process::id());
    let mut trace = trace::Trace::new(&args.atrace_filename.unwrap(), &scratch_filename)?;
    summary::summarize_allocations(&mut trace, !report_mode)?;

    if report_mode {
        report::generate_report(trace)?;
    } else {
        ui::main_loop(trace, args.report_perf);
    }

    if let Err(err) = std::fs::remove_file(&scratch_filename) {
        eprintln!("Can't remove scratch file: {:?}", err);
    }

    Ok(())
}
