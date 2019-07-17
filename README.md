
# backtrace-string

This crate provides a way to get a backtrace string. It uses the
[`backtrace` crate](https://crates.io/crates/backtrace) internally
to generate a backtrace and then walks through all backtrace frames
using rustc_demangel to create a human readable backtrace returning
it as a string.

Note that it is currently mainly meant to be used in a panic hook,
using it outside of it might work, but might also lead to unexpected
trimming of the backtrace.


```rust
use backtrace_string::create_backtrace;

fn main() {
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("Our own panic handler, yay");
        // This is separate from the backtrace.
        if let Some(loc) = panic_info.location() {
            eprintln!("Panic at: {}:{}:{}", loc.file(), loc.line(), loc.column());
        }
        // Print the backtrace (due to inlining there
        // might not be much to see :/ )
        eprintln!("Backtrace: {}", create_backtrace());
    }));

    do_that_thing(0);
}

// Arbitrary function which uses recursion to:
// 1. have something to show in the backtrace
// 2. prevent rust/llvm from completely inlining it and min into
//    the rust internal startup function.
fn do_that_thing(x: u32)  {
    if x > 4 {
        panic!("and run away...");
    } else if x > 4 {
        return;
    }
    do_that_thing(x+1);
}
```

Outputs:

```
Our own panic handler, yay
Panic at: examples/readme.rs:24:9
Backtrace:
   0: readme::do_that_thing::hbb6093a4a26437d3
          at examples/readme.rs:24
   1: readme::do_that_thing::hbb6093a4a26437d3
          at examples/readme.rs:28
   2: readme::do_that_thing::hbb6093a4a26437d3
          at examples/readme.rs:28
   3: readme::do_that_thing::hbb6093a4a26437d3
          at examples/readme.rs:28
   4: readme::do_that_thing::hbb6093a4a26437d3
          at examples/readme.rs:28
   5: readme::do_that_thing::hbb6093a4a26437d3
          at examples/readme.rs:28
   6: readme::main::h94e6e84dae2a4c86
          at examples/readme.rs:15
   7: std::rt::lang_start::{{closure}}::h6f382580a8fe3059
          at /rustc/a53f9df32fbb0b5f4382caaad8f1a46f36ea887c/src/libstd/rt.rs:64
   8: std::rt::lang_start_internal::{{closure}}::h3a7adfabc7c47a5f
          at src/libstd/rt.rs:49
      std::panicking::try::do_call::hc3d8373a0b215f51
          at src/libstd/panicking.rs:293
   9: __rust_maybe_catch_panic
          at src/libpanic_unwind/lib.rs:85
  10: std::panicking::try::hfb06c315006b63ac
          at src/libstd/panicking.rs:272
      std::panic::catch_unwind::h6cd8469da971482b
          at src/libstd/panic.rs:394
      std::rt::lang_start_internal::he5218c8b95d395f2
          at src/libstd/rt.rs:48
  11: std::rt::lang_start::h80ca1e889e87da88
          at /rustc/a53f9df32fbb0b5f4382caaad8f1a46f36ea887c/src/libstd/rt.rs:64
  12: main
          at address 0x559f6430a8e9
```