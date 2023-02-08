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
use object::{Object, ObjectSegment, ObjectSymbol};
use std::collections::{BTreeMap, HashMap};

// A reference to a function mapped into the traced process.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    // The name of the function.
    pub name: String,

    // The address in the traced process's address space.
    pub address: u64,

    // The length of the function in bytes.
    pub size: u64,
}

// An index of symbol names and addresses to which those symbols resolve.
#[derive(Debug)]
pub struct SymbolIndex {
    // The map from symbol name to information about the symbol.
    pub symbols_by_name: HashMap<String, Vec<SymbolInfo>>,

    // A map from address to symbol info.
    pub symbols_by_address: BTreeMap<u64, SymbolInfo>,
}

impl SymbolIndex {
    // Start a new empty symbol index.
    pub fn new() -> SymbolIndex {
        SymbolIndex {
            symbols_by_name: HashMap::new(),
            symbols_by_address: BTreeMap::new(),
        }
    }

    // Check whether a particular symbol falls within the address range
    // mapped by a ProcessMapEntry, and if so, then store the relevant
    // address in the symbol map.
    fn add_symbol(
        &mut self,
        entry: &process_map::ProcessMapEntry,
        address_offset: i64,
        symbol: &object::Symbol,
    ) {
        match symbol.name() {
            Ok(name) => {
                let sym_address = (symbol.address() as i64 - address_offset) as u64;
                let size = symbol.size();

                if sym_address >= entry.offset
                    && sym_address < entry.offset + (entry.end - entry.begin)
                {
                    let address = entry.begin + sym_address - entry.offset;
                    let symbol_info = SymbolInfo {
                        name: name.to_owned(),
                        address,
                        size,
                    };

                    if !self.symbols_by_name.contains_key(name) {
                        self.symbols_by_name.insert(name.to_owned(), Vec::new());
                    }
                    let entry = self.symbols_by_name.get_mut(name).unwrap();
                    entry.push(symbol_info.clone());

                    self.symbols_by_address.insert(address, symbol_info);
                }
            }
            Err(_) => (),
        }
    }

    // Add symbols from a parsed object file to the symbol index.
    fn add_elf_symbols(&mut self, entry: &process_map::ProcessMapEntry, elf: &object::File) {
        let mut address_offset: Option<i64> = None;

        for segment in elf.segments() {
            let range = segment.file_range();

            if range.0 == entry.offset {
                address_offset = Some((segment.address() - range.0) as i64);
            }
        }

        if address_offset == None {
            return;
        }

        // Iterate through all symbols in the binary, adding
        // them to the symbol map if they are in the mmap
        // range.
        for symbol in elf.symbols() {
            self.add_symbol(entry, address_offset.unwrap(), &symbol);
        }

        // Similarly, but for dynamic symbols.
        for symbol in elf.dynamic_symbols() {
            self.add_symbol(entry, address_offset.unwrap(), &symbol);
        }
    }

    // Add all the symbols for a particluar mmaped range of an executable
    // which has been mapped into a traced process.
    pub fn add_entry_symbols(&mut self, entry: &process_map::ProcessMapEntry) {
        match &entry.filename {
            Some(filename) => match std::fs::read(filename.clone()) {
                Ok(elf_data) => match object::File::parse(&*elf_data) {
                    Ok(elf) => {
                        self.add_elf_symbols(entry, &elf);
                    }
                    Err(_) => (),
                },
                _ => (),
            },
            None => (),
        }
    }

    // Given the process map of a traced process, add entries for all symbols
    // found in the executables mapped into the process's address space.
    pub fn add_symbols(&mut self, process_map: &process_map::ProcessMap) {
        for entry in &process_map.entries {
            self.add_entry_symbols(&entry);
        }
    }

    // Get function name by address.  We'll try a few symbols which start
    // proir to the address we are checking, as glibc likes to leave GLIBC
    // symbols near the function name.
    pub fn get_function_by_address(&self, address: u64) -> Option<SymbolInfo> {
        let mut tries = 0;
        let mut symbols_by_range = self.symbols_by_address.range(..address + 1);
        while let Some((_, info)) = symbols_by_range.next_back() {
            if address - info.address <= info.size {
                return Some(info.clone());
            }

            tries += 1;
            if tries >= 4 {
                break;
            }
        }

        return None;
    }
}
