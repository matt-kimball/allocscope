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

use crate::process_map;
use crate::symbol_index;
use libunwind_sys;
use std::error::Error;
use std::path;

// An entry representing a stack frame in a stack backtrace.
#[derive(Debug)]
pub struct StackEntry {
    // The instruction address for this frame.
    pub address: u64,

    // The name of the function containing the address for this frame.
    pub name: String,

    // The offset from the start of the function.
    pub offset: u64,
}

// A wrapper for libunwind's conception of a remote address space.
pub struct AddressSpace {
    // The libunwind handle for the address space.
    handle: libunwind_sys::unw_addr_space_t,
}

impl AddressSpace {
    // Create a new libunwind address space object.
    fn new(
        accessors: *mut libunwind_sys::unw_accessors_t,
        byteorder: libc::c_int,
    ) -> Result<AddressSpace, Box<dyn Error>> {
        unsafe {
            let handle = libunwind_sys::unw_create_addr_space(accessors, byteorder);
            if handle == std::ptr::null_mut() {
                Err("failure to create libunwind address space")?
            }

            Ok(AddressSpace { handle })
        }
    }

    // Create a new libunwind address space using libunwind's built-in ptrace
    // accessors to get at the memory in that address space.
    pub fn new_upt() -> Result<AddressSpace, Box<dyn Error>> {
        unsafe { AddressSpace::new(&mut libunwind_sys::_UPT_accessors, 0) }
    }
}

impl Drop for AddressSpace {
    // Destroy the address space when dropped.
    fn drop(&mut self) {
        unsafe {
            libunwind_sys::unw_destroy_addr_space(self.handle);
        }
    }
}

// ptrace accessors as implemented by libunwind.
pub struct UPTAccessors {
    // The raw pointer to the accessor functions.
    handle: *mut std::ffi::c_void,
}

impl UPTAccessors {
    // Create a new accessor using libunwind's built-in ptrace accessors.
    pub fn new(pid: i32) -> Result<UPTAccessors, Box<dyn Error>> {
        unsafe {
            let handle = libunwind_sys::_UPT_create(pid as i32);
            if handle == std::ptr::null_mut() {
                Err("failure to create libunwind UPT accessors")?
            }

            Ok(UPTAccessors { handle })
        }
    }
}

impl Drop for UPTAccessors {
    // Destroy the accessors when dropped.
    fn drop(&mut self) {
        unsafe {
            libunwind_sys::_UPT_destroy(self.handle);
        }
    }
}

// Collect the current stack from a stopped traced thread using libunwind.
pub fn collect_stack(
    process_map: &process_map::ProcessMap,
    symbol_index: &symbol_index::SymbolIndex,
    address_space: &AddressSpace,
    upt: &UPTAccessors,
) -> Result<Vec<StackEntry>, Box<dyn Error>> {
    let mut stack = Vec::<StackEntry>::new();

    unsafe {
        let mut cursor =
            std::mem::MaybeUninit::<libunwind_sys::unw_cursor_t>::zeroed().assume_init();
        if libunwind_sys::unw_init_remote(&mut cursor, address_space.handle, upt.handle) != 0 {
            Err("failure to initialize libunwind remote address space")?
        }

        loop {
            let mut address: libunwind_sys::unw_word_t = 0;

            // Get the address for this stack frame.
            if libunwind_sys::unw_get_reg(
                &mut cursor,
                libunwind_sys::UNW_TDEP_IP as i32,
                &mut address,
            ) != 0
            {
                Err("failure to unwind instruction pointer")?
            }

            let mut offset: libunwind_sys::unw_word_t = 0;
            let mut name: String = "".to_string();
            if let Some(symbol) = symbol_index.get_function_by_address(address) {
                name = symbol.name.clone();
                offset = address - symbol.address;
            } else {
                // If we can't resolve the address to a function, instead use
                // the filename from which the instructions are mapped.
                if let Some(entry) = process_map.entry_for_address(address) {
                    if let Some(filename) = &entry.filename {
                        let path = path::Path::new(filename);
                        if let Some(basename) = path.file_name() {
                            if let Some(basename_str) = basename.to_str() {
                                name = format!("[{}]", basename_str);
                                offset = address - entry.begin + entry.offset;
                            }
                        }
                    }
                }
            }

            stack.push(StackEntry {
                address,
                name,
                offset,
            });

            let step_result = libunwind_sys::unw_step(&mut cursor);
            if step_result < 0 {
                Err("failure to step libunwind stack")?
            } else if step_result == 0 {
                break;
            }
        }
    }

    Ok(stack)
}
