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
