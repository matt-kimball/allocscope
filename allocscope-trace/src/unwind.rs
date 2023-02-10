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
use std::collections::HashMap;
use std::error::Error;
use std::path;

// Accessors we will pass to libunwind for crawling the stack.  We want
// to override the 'access_mem' accessor because it is the most critical for
// performance, and we can write a faster implementation than the default
// _UPT_access_mem implementation.
const UNWIND_ACCESSORS: libunwind_sys::unw_accessors_t = libunwind_sys::unw_accessors_t {
    find_proc_info: Some(libunwind_sys::_UPT_find_proc_info),
    put_unwind_info: Some(libunwind_sys::_UPT_put_unwind_info),
    get_dyn_info_list_addr: Some(libunwind_sys::_UPT_get_dyn_info_list_addr),
    access_mem: Some(unwind_access_mem),
    access_reg: Some(libunwind_sys::_UPT_access_reg),
    access_fpreg: Some(libunwind_sys::_UPT_access_fpreg),
    resume: Some(libunwind_sys::_UPT_resume),
    get_proc_name: Some(libunwind_sys::_UPT_get_proc_name),
};

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
        accessors: *const libunwind_sys::unw_accessors_t,
        byteorder: libc::c_int,
    ) -> Result<AddressSpace, Box<dyn Error>> {
        unsafe {
            let handle = libunwind_sys::unw_create_addr_space(
                accessors as *mut libunwind_sys::unw_accessors_t,
                byteorder,
            );
            if handle == std::ptr::null_mut() {
                Err("failure to create libunwind address space")?
            }

            Ok(AddressSpace { handle })
        }
    }

    // Create a new libunwind address space using libunwind's built-in ptrace
    // accessors to get at the memory in that address space.
    pub fn new_upt() -> Result<AddressSpace, Box<dyn Error>> {
        AddressSpace::new(&UNWIND_ACCESSORS, 0)
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
pub struct UPTContext {
    // The raw pointer to the accessor functions.
    handle: *mut std::ffi::c_void,
}

impl UPTContext {
    // Create a new accessor using libunwind's built-in ptrace accessors.
    pub fn new(pid: i32) -> Result<UPTContext, Box<dyn Error>> {
        unsafe {
            let handle = libunwind_sys::_UPT_create(pid as i32);
            if handle == std::ptr::null_mut() {
                Err("failure to create libunwind UPT accessors")?
            }

            Ok(UPTContext { handle })
        }
    }
}

impl Drop for UPTContext {
    // Destroy the accessors when dropped.
    fn drop(&mut self) {
        unsafe {
            libunwind_sys::_UPT_destroy(self.handle);
        }
    }
}

// A context for crawling the stack.  We can speed up stack crawling
// significantly if we cache values to minimize the number of ptrace()
// calls to access another process's memory.
struct CrawlContext {
    // A cache mapping addresses to value.
    cache: HashMap<u64, u64>,

    // The most recent address read.
    previous_address: u64,

    // The most recent value read.
    previous_value: u64,
}

// A global context for crawling the stack is gross, but it is the most
// practical option, because we want to use it from libunwind's accessor
// functions.  libunwind has a mechanism for passing a context pointer to
// the callbacks, but if we want to use the standard _UPT callbacks for some
// of the accessors then we need to pass the standard _UPT context to them.
// We can't just use a wrapper, because the _UPT accessor callbacks are
// reentrant.  That is to say, some of the standard accessors will call our
// 'access_mem' accessor with whatever context we pass them.
//
// So, global variable, and assume we will only ever be accessing it from one
// thread.
static mut CRAWL_CONTEXT: Option<CrawlContext> = None;

impl CrawlContext {
    // Create a new context with an empty cache.
    fn new() -> CrawlContext {
        CrawlContext {
            cache: HashMap::new(),
            previous_address: 0,
            previous_value: 0,
        }
    }
}

// Get a function name and offset given and address in the traced process.
fn get_function_by_address(
    process_map: &process_map::ProcessMap,
    symbol_index: &symbol_index::SymbolIndex,
    address: u64,
) -> (String, u64) {
    let mut offset = 0;
    let mut name = "".to_string();

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

    (name, offset)
}

// Collect the stack from the traced process.  Assumes we have exclusive
// access to the global CRAWL_CONTEXT.
unsafe fn collect_stack_non_threadsafe(
    process_map: &process_map::ProcessMap,
    symbol_index: &symbol_index::SymbolIndex,
    address_space: &AddressSpace,
    upt: &UPTContext,
) -> Result<Vec<StackEntry>, Box<dyn Error>> {
    let mut stack = Vec::<StackEntry>::new();

    let mut cursor = std::mem::MaybeUninit::<libunwind_sys::unw_cursor_t>::zeroed().assume_init();
    if libunwind_sys::unw_init_remote(&mut cursor, address_space.handle, upt.handle) != 0 {
        Err("failure to initialize libunwind remote address space")?
    }

    loop {
        let mut address: libunwind_sys::unw_word_t = 0;

        // Get the address for this stack frame.
        if libunwind_sys::unw_get_reg(&mut cursor, libunwind_sys::UNW_TDEP_IP as i32, &mut address)
            != 0
        {
            Err("failure to unwind instruction pointer")?
        }

        let (name, offset) = get_function_by_address(process_map, symbol_index, address);
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

    Ok(stack)
}

// Collect the current stack from a stopped traced thread using libunwind.
// Given this uses the global CRAWL_CONTEXT, it is only safe if it is called
// by one thread.
pub fn collect_stack(
    process_map: &process_map::ProcessMap,
    symbol_index: &symbol_index::SymbolIndex,
    address_space: &AddressSpace,
    upt: &UPTContext,
) -> Result<Vec<StackEntry>, Box<dyn Error>> {
    unsafe {
        // The assumption is that we only have one thread using CRAWL_CONTEXT.
        // If we had multiple threads calling collect_stack, this would not be
        // threadsafe.
        CRAWL_CONTEXT = Some(CrawlContext::new());

        let result = collect_stack_non_threadsafe(process_map, symbol_index, address_space, upt);

        CRAWL_CONTEXT = None;

        result
    }
}

// Read memory values from the traced process, but use a cache to retreive
// them to speed up access.
unsafe extern "C" fn unwind_access_mem(
    address_space: libunwind_sys::unw_addr_space_t,
    address: libunwind_sys::unw_word_t,
    value: *mut libunwind_sys::unw_word_t,
    write: i32,
    context: *mut std::ffi::c_void,
) -> i32 {
    if write == 0 {
        if let Some(crawl) = &mut CRAWL_CONTEXT {
            // It turns out that libunwind will repeatedly ask for the same
            // memory value, so it is a win to check if we are getting the
            // most recently retrieved value.
            if address == crawl.previous_address {
                *value = crawl.previous_value;
            } else if let Some(cache_value) = crawl.cache.get(&address) {
                // Otherwise, use the cached value if it is available.
                crawl.previous_address = address;
                crawl.previous_value = *cache_value;
                *value = *cache_value;
            } else {
                let mut read_value: u64 = 0;

                // The fallback option is to actually use ptrace() to read
                // from the traced process's memory.
                let result = libunwind_sys::_UPT_access_mem(
                    address_space,
                    address,
                    &mut read_value,
                    write,
                    context,
                );
                if result != 0 {
                    return result;
                }

                crawl.cache.insert(address, read_value);
                crawl.previous_address = address;
                crawl.previous_value = read_value;
                *value = read_value;
            }

            0
        } else {
            libunwind_sys::_UPT_access_mem(address_space, address, value, write, context)
        }
    } else {
        libunwind_sys::_UPT_access_mem(address_space, address, value, write, context)
    }
}
