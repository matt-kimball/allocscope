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

use std::error::Error;

// Parsed commandline arguments.
pub struct CommandLineArguments {
    // Filename from which to read the trace.
    pub atrace_filename: Option<String>,

    // If true, we should generate a text (non-ncurses) report.
    pub report_mode: bool,

    // If true, we should show performance statistics i nthe ncurses UI.
    pub report_perf: bool,

    // If true, print the version of the tool and exit.
    pub report_version: bool,

    // If true, print the commandline help text and exit.
    pub show_help: bool,
}

// Print the commandline help text.
pub fn show_help() {
    println!(
        "Usage: allocscope-view [OPTIONS] [ATRACE-FILENAME]

    -r, --report    Generate text report to stdout
    -v, --version   Report version
"
    );
}

// Print the version of the build.
pub fn report_version() {
    println!("allocscope-view {}", env!("CARGO_PKG_VERSION"));
}

impl CommandLineArguments {
    // Parse the commandline.
    pub fn parse(
        args: &mut dyn Iterator<Item = String>,
    ) -> Result<CommandLineArguments, Box<dyn Error>> {
        let mut atrace_filename: Option<String> = None;
        let mut report_mode = false;
        let mut report_perf = false;
        let mut report_version = false;
        let mut show_help = false;

        for token in args.skip(1) {
            if token.chars().next() == Some('-') {
                if token.chars().nth(1) == Some('-') {
                    match token.as_str() {
                        "--help" => show_help = true,
                        "--perf" => report_perf = true, // Undocumented command for development.
                        "--report" => report_mode = true,
                        "--version" => report_version = true,
                        _ => {
                            eprintln!("Unrecognized argument: {}", token);
                            show_help = true;
                        }
                    }
                } else {
                    for char in token.chars().skip(1) {
                        match char {
                            'h' => show_help = true,
                            'r' => report_mode = true,
                            'v' => report_version = true,
                            _ => {
                                eprintln!("Unrecognized flag: {}", char);
                                show_help = true;
                            }
                        }
                    }
                }
            } else if atrace_filename.is_none() && atrace_filename.is_none() {
                atrace_filename = Some(token);
            } else {
                eprintln!("Spurious argument: {}", token);
                show_help = true;
            }
        }

        Ok(CommandLineArguments {
            atrace_filename: atrace_filename,
            report_mode,
            report_perf,
            report_version,
            show_help,
        })
    }
}
