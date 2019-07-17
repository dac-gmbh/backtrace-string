
# backtrace-string

This crate provides a way to get a backtrace string. It uses the
[`backtrace` crate](https://crates.io/crates/backtrace) internally
to generate a backtrace and then walks through all backtrace frames
using rustc_demangel to create a human readable backtrace returning
it as a string.



```rust
use backtrace_string::create_backtrace;

fn main() {
    println!("{}", create_backtrace());
}
```