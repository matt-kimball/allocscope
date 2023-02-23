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
use std::error::Error;

// Trace a simple C program which does allocations in a loop.
#[test]
fn test_basic_trace() -> Result<(), Box<dyn Error>> {
    let leaf = integration_test::build_and_get_leaf("loop.c")?;

    assert_eq!(leaf.bytes, "1024k");
    assert_eq!(leaf.blocks, "1024");
    assert_eq!(leaf.leaks, "0");
    assert_eq!(leaf.tree.len(), 8);
    assert!(leaf.name.contains("malloc"));

    Ok(())
}

// Trace a program which uses realloc for allocation.
#[test]
fn test_realloc() -> Result<(), Box<dyn Error>> {
    let leaf = integration_test::build_and_get_leaf("realloc.c")?;

    assert_eq!(leaf.bytes, "512k");
    assert_eq!(leaf.blocks, "20");
    assert_eq!(leaf.leaks, "0");
    assert_eq!(leaf.tree.len(), 8);
    assert!(leaf.name.contains("realloc"));

    Ok(())
}

// Trace a C++ program, and verify that the function name demangling works.
#[test]
fn test_cplusplus() -> Result<(), Box<dyn Error>> {
    let line = integration_test::build_and_get_named("cplusplus.cc", "iter(unsigned long)")?;

    assert_eq!(line.bytes, "1024k");
    assert_eq!(line.blocks, "1024");
    assert_eq!(line.leaks, "0");

    Ok(())
}

// Trace a Rust program, and verify we can demangle Rust function names.
#[test]
fn test_rust() -> Result<(), Box<dyn Error>> {
    let line = integration_test::build_and_get_named("rust.rs", "rust::iter")?;

    assert_eq!(line.bytes, "1024k");
    assert_eq!(line.blocks, "1024");
    assert_eq!(line.leaks, "0");

    Ok(())
}

// Trace a program which spawns several threads.  Those threads all do
// allocations using the same code.
#[test]
fn test_threaded_trace() -> Result<(), Box<dyn Error>> {
    let leaf = integration_test::build_and_get_leaf("threaded.c")?;

    // Not explicitly checking leaf.bytes, because multiple threads are
    // allocating simultaneously, and the exact peak value is a race.
    assert_eq!(leaf.blocks, "800");
    assert_eq!(leaf.leaks, "0");
    assert_eq!(leaf.tree.len(), 10);
    assert!(leaf.name.contains("malloc"));

    Ok(())
}
