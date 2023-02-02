# allocscope
### a memory tracking tool

allocscope is a tool for tracking down where the most egregiously large allocations are occurring
in a C, C++ or Rust codebase.

It is composed of two commands:

`allocscope-trace` attaches to another process as a debugger.  By using breakpoints on memory
allocation functions such as `malloc` it tracks allocations made by that process.

`allocscope-view` reads a trace file produced by `allocscope-trace`.  It presents a summary of all
allocations made in a call tree format, which can be sorted by largest concurrent allocation,
total number of blocks, or number of unfreed allocation blocks.

# License

allocscope is licensed GNU General Public License version 3.
