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
use std::path;

// Parsed commandline arguments.
pub struct CommandLineArguments {
    // Filename to use for the trace.
    pub atrace_filename: String,

    // The commandline for the process to trace.
    pub command: Vec<String>,

    // The process-id of a running process to which to attach the trace.
    pub target_pid: Option<u32>,

    // If true, print the version of the tool and exit.
    pub report_version: bool,

    // If true, print the commandline help text and exit.
    pub show_help: bool,
}

// Print the commandline help text.
pub fn show_help() {
    println!(
        "Usage: allocscope-trace [OPTIONS] [COMMAND]

    -o, --output FILE   Record trace to given filename
    -p, --pid TARGET    Attach to running process
    -v, --version       Report version
"
    );
}

// Print the version of the build.
pub fn report_version() {
    println!("allocscope-trace {}", env!("CARGO_PKG_VERSION"));
}

// Given a command to trace, generate an appropriate filename for the trace.
fn get_trace_filename_from_command(command: &Vec<String>) -> Result<String, Box<dyn Error>> {
    if command.len() > 0 {
        let path = path::Path::new(&command[0]);
        if let Some(basename) = path.file_name() {
            Ok(format!(
                "{}.atrace",
                basename.to_str().ok_or("invalid command name")?
            ))
        } else {
            Ok("alloc-trace.atrace".to_string())
        }
    } else {
        Ok("alloc-trace.atrace".to_string())
    }
}

impl CommandLineArguments {
    // Parse the commandline.
    pub fn parse(
        args: &mut dyn Iterator<Item = String>,
    ) -> Result<CommandLineArguments, Box<dyn Error>> {
        let mut atrace_filename: Option<String> = None;
        let mut command: Vec<String> = Vec::new();
        let mut target_pid: Option<u32> = None;
        let mut show_help = false;
        let mut command_started = false;
        let mut report_version = false;

        let mut expect_pid = false;
        let mut expect_atrace_filename = false;
        for token in args.skip(1) {
            let mut consumed_token = false;

            // If the target command has already started, assume any flag
            // arguments are for the target, not us.
            if !command_started {
                if token.chars().next() == Some('-') {
                    consumed_token = true;

                    if token.chars().nth(1) == Some('-') {
                        match token.as_str() {
                            "--help" => show_help = true,
                            "--output" => expect_atrace_filename = true,
                            "--pid" => expect_pid = true,
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
                                'o' => expect_atrace_filename = true,
                                'p' => expect_pid = true,
                                'v' => report_version = true,
                                _ => {
                                    eprintln!("Unrecognized flag: {}", char);
                                    show_help = true;
                                }
                            }
                        }
                    }
                } else if expect_pid {
                    consumed_token = true;
                    expect_pid = false;
                    target_pid = match token.parse::<u32>() {
                        Ok(target_pid) => Some(target_pid),
                        Err(_) => Err(format!("invalid target PID: {}", token))?,
                    };
                } else if expect_atrace_filename {
                    consumed_token = true;
                    expect_atrace_filename = false;
                    atrace_filename = Some(token.clone());
                }
            }

            if !consumed_token {
                command.push(token.clone());
                command_started = true;
            }
        }

        Ok(CommandLineArguments {
            atrace_filename: match atrace_filename {
                Some(filename) => filename,
                None => get_trace_filename_from_command(&command)?,
            },
            command,
            target_pid,
            report_version,
            show_help,
        })
    }
}
