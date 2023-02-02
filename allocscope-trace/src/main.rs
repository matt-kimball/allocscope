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

mod breakpoint;
mod commandline;
mod context;
mod hooks;
mod process_map;
mod ptrace;
mod record;
mod symbol_index;
mod trace;
mod unwind;

use std::error::Error;

// The main entry point for allocscope-trace.
fn main() -> Result<(), Box<dyn Error>> {
    let args = commandline::CommandLineArguments::parse(&mut std::env::args())?;
    if args.report_version {
        commandline::report_version();
        return Ok(());
    }
    if args.show_help {
        commandline::show_help();
        return Ok(());
    }

    if args.target_pid.is_some() {
        let record = record::TraceRecord::new(&args.atrace_filename)?;
        trace::trace_pid(record, args.target_pid.unwrap())?;
    } else if args.command.len() > 0 {
        let record = record::TraceRecord::new(&args.atrace_filename)?;
        trace::trace_command(record, &args.command)?;
    } else {
        commandline::show_help();
    }

    Ok(())
}
