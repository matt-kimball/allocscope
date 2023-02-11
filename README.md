![allocscope banner](https://allocscope.com/banner.png)

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

# Installing prebuilt binaries

The easiest way to get started with allocscope is to install prebuilt binaries.

To install the latest version:

`curl -s https://allocscope.com/install.sh | sudo sh`

Currently only Linux on x86_64 processors is supported, but I'd like to support more operating systems
and processors in the future.

# Building from source

On recent Ubuntu releases, allocscope can be built from source with the following sequence
of commands:

```
apt-get update
apt-get install cargo git libclang-dev libiberty-dev libncurses-dev libsqlite3-dev libunwind-dev
git clone https://github.com/matt-kimball/allocscope.git
cd allocscope
cargo install --path allocscope-trace
cargo install --path allocscope-view
```

# Support development

If you find allocscope useful, please consider supporting development.

Visit https://allocscope.com/support

# License

allocscope is licensed GNU General Public License version 3.
