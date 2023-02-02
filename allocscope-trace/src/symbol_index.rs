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
use object::{Object, ObjectSymbol};
use std::collections::HashMap;

// A list of addresses to which a particular symbol name resolves.
#[derive(Debug)]
pub struct SymbolEntry {
    // The addresses.
    pub addresses: Vec<u64>,
}

// An index of symbol names and addresses to which those symbols resolve.
#[derive(Debug)]
pub struct SymbolIndex {
    // The map from symbol name to list of addresses.
    pub symbols: HashMap<String, SymbolEntry>,
}

impl SymbolIndex {
    // Start a new empty symbol index.
    pub fn new() -> SymbolIndex {
        let symbols = HashMap::new();
        SymbolIndex { symbols }
    }

    // Check whether a particular symbol falls within the address range
    // mapped by a ProcessMapEntry, and if so, then store the relevant
    // address in the symbol map.
    fn add_symbol(&mut self, entry: &process_map::ProcessMapEntry, symbol: &object::Symbol) {
        match symbol.name() {
            Ok(name) => {
                if symbol.address() >= entry.offset
                    && symbol.address() < entry.offset + (entry.end - entry.begin)
                {
                    let address = entry.begin + symbol.address() - entry.offset;
                    if !self.symbols.contains_key(name) {
                        let addresses = Vec::new();
                        self.symbols
                            .insert(name.to_owned(), SymbolEntry { addresses });
                    }
                    let entry = self.symbols.get_mut(name).unwrap();
                    entry.addresses.push(address);
                }
            }
            Err(_) => (),
        }
    }

    // Add all the symbols for a particluar mmaped range of an executable
    // which has been mapped into a traced process.
    pub fn add_entry_symbols(&mut self, entry: &process_map::ProcessMapEntry) {
        match &entry.filename {
            Some(filename) => match std::fs::read(filename.clone()) {
                Ok(elf_data) => match object::File::parse(&*elf_data) {
                    Ok(elf) => {
                        // Iterate through all symbols in the binary, adding
                        // them to the symbol map if they are in the mmap
                        // range.
                        for symbol in elf.symbols() {
                            self.add_symbol(entry, &symbol);
                        }

                        // Similarly, but for dynamic symbols.
                        for symbol in elf.dynamic_symbols() {
                            self.add_symbol(entry, &symbol);
                        }
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
}
