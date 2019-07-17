//! `backtrace-string` generates a backtrace as a human readable string.
//!
//! This library uses the [`backtrace` crate](https://crates.io/crates/backtrac)
//! to generate a backtrace and then converts it to a human readable string
//! by demangleing names, doing some formating etc.
//!
//! Note that for this is meant to be used in panic hooks only.

use {
    backtrace::{Backtrace, BacktraceFrame},
    rustc_demangle::demangle,
    std::{
        borrow::Cow,
        fmt::Write,
        path::{Path, PathBuf},
    },
};


/// Creates a backtrace and calls [`format_backtrace()`] on it.
///
///[`format_backtrace()`]: fn.format_backtrace.html
pub fn create_backtrace() -> String {
    let mut bt = Backtrace::new();
    format_backtrace(&mut bt)
}

/// Outputs the backtrace as a human readable string.
///
/// **Warning the formating for now is focused on calls from inside a panic
/// hook, calling it from outside might not work as expected until more
/// scenarios are covered and tested**
///
/// Note that this does some rust specific backtrace shortening, mainly
/// some frames from the panic handling functionality are skipped over
/// and some rust paths to crates get shortened.
pub fn format_backtrace(bt: &mut Backtrace) -> String {
    bt.resolve();

    let mut out = String::from("\n");
    for (i, frame) in filter_frames(bt.frames()).enumerate() {
        format_frame_into(&mut out, i, frame);
    }
    out
}


fn format_frame_into(out: &mut String, index: usize, frame: &BacktraceFrame) {
    write!(out, "{:4}:", index).unwrap();

    let mut last_symbol = None;
    for symbol in frame.symbols() {
        let name = demangle(
            symbol
                .name()
                .and_then(|name| name.as_str())
                .unwrap_or("<unknown>"),
        )
        .to_string();

        match last_symbol.take() {
            None => {
                write!(out, " {}", name).unwrap();
                last_symbol = Some(name);
            }
            Some(ref sym) if sym != &name => {
                write!(out, "\n      {}", name).unwrap();
                last_symbol = Some(name);
            }

            // FIXME: Make less ugly once "cannot bind by-move into a pattern guard"
            // is fixed in rustc (post-NLL I believe).
            old => last_symbol = old,
        }

        write!(out, "\n          at ").unwrap();
        let path = symbol.filename().map(clean_path);
        match (path, symbol.addr(), symbol.lineno()) {
            (Some(path), _, Some(line)) => write!(out, "{}:{}", path.display(), line).unwrap(),
            (Some(path), _, _) => write!(out, "{}", path.display()).unwrap(),
            (None, Some(addr), _) => write!(out, "address {:p}", addr).unwrap(),
            (None, None, _) => write!(out, "<unknown>").unwrap(),
        }
    }

    writeln!(out).unwrap();
}

/// "Opportunistic" filtering of frames.
///
/// This will remove frames we're sure are irrelevant. This mostly includes stuff inside the
/// `backtrace` crate, and, on the other end of the stack, Rust runtime startup code.
///
/// This is "opportunistic" because it will simply not trim any frames if it isn't sure that the
/// frames are really irrelevant. Still, if the backtraces act up, try disabling this function.
fn filter_frames<'a>(frames: &'a [BacktraceFrame]) -> impl Iterator<Item = &'a BacktraceFrame> {
    // The start of the backtrace (most recent calls) are inside the `backtrace` crate, our panic
    // hook, and `std::panicking`. We search the first 10 frames for `std::panicking::*` symbols and
    // trim just below them.

    // `Take` cannot implement `DoubleEndedIterator` and so `rposition` doesn't work on it. Get the
    // subslice manually.
    let fr = if frames.len() > 10 {
        &frames[..10]
    } else {
        frames
    };
    let start_index = fr.iter().rposition(|frame| {
        frame_contains_symbol(frame, |sym| {
            // At some point the `std::panicking` prefix got lost, so we also check for a bare
            // `panic_fmt` symbol.
            sym == "panic_fmt" || sym.starts_with("std::panicking")
        })
    });

    // The end of the backtrace contains libc startup, Rust runtime startup, possibly the thread
    // creation code, catch_panic, and, importantly, the `__rust_begin_short_backtrace` symbol.
    let end_index = frames
        .iter()
        .enumerate()
        .rev()
        .find(|(_, frame)| {
            frame_contains_symbol(frame, |sym| sym.contains("__rust_begin_short_backtrace"))
        })
        .map(|(i, _)| i);

    let start_index = start_index.and_then(|s| {
        if end_index.as_ref().map(|e| s >= *e).unwrap_or(false) {
            None
        } else {
            Some(s)
        }
    });

    frames
        .iter()
        .enumerate()
        .filter(move |(i, _)| {
            let after_start = start_index.map(|idx| *i > idx).unwrap_or(true);
            let before_end = end_index.map(|idx| *i < idx).unwrap_or(true);
            after_start && before_end
        })
        .map(|(_, frame)| frame)
}

/// Returns whether `frame` contains a symbol name for which `pred` returns `true`.
fn frame_contains_symbol(frame: &BacktraceFrame, mut pred: impl FnMut(&str) -> bool) -> bool {
    frame.symbols().iter().any(|sym| {
        sym.name()
            .and_then(|name| name.as_str())
            .map(|name| pred(&demangle(name).to_string()))
            .unwrap_or(false)
    })
}


/// Opportunistic file path shortening.
///
/// While references to the final crate and the standard library seem to use relative paths,
/// references to crates.io dependencies use absolute paths, which makes them hard to read
/// (especially when using futures and tokio in debug builds). This function shortens those paths
/// to start with the crate's directory instead.
fn clean_path(p: &Path) -> Cow<Path> {
    // Relative paths point to the final crate or the standard library. Absolute paths point to
    // crates.io dependencies. Those are the paths we want to shorten.
    if p.is_absolute() {
        // We rely on Cargo paths to contain `github.com-*`, and cut that part off.
        p.iter()
            .position(|component| {
                component
                    .to_str()
                    .map(|s| s.starts_with("github.com-"))
                    .unwrap_or(false)
            })
            .map(|i| {
                // Remove the beginning of the path, including the `github.com-*` part.
                p.iter().skip(i + 1).collect::<PathBuf>().into()
            })
            .unwrap_or_else(|| {
                // Path doesn't contain "github.com-", don't modify it.
                p.into()
            })
    } else {
        p.into()
    }
}


#[cfg(test)]
mod tests {
    use lazy_static::lazy_static;

    use std::{
        collections::HashMap,
        panic::{self, PanicInfo, UnwindSafe},
        sync::{Mutex, Arc, atomic::{AtomicUsize, Ordering}},
        cell::Cell,
    };

    type PanicHookFn = dyn Fn(&PanicInfo) + Sync + Send + 'static;

    fn with_panic_hook(hook: Box<PanicHookFn>, func: impl FnOnce() + UnwindSafe) {
        let reset_id = set_panic_hook(hook);
        let _ = panic::catch_unwind(func);
        unset_panic_hook(reset_id);

        ///-----------

        fn set_panic_hook(hook: Box<PanicHookFn>) -> usize {
            let hook_id = set_hook_id();
            HOOKS.lock().unwrap().insert(hook_id, hook);
            hook_id
        }
        fn unset_panic_hook(id: usize) {
            HOOKS.lock().unwrap().remove(&id);
        }
        fn set_hook_id() -> usize {
            let id = HOOK_ID_GEN.fetch_add(1, Ordering::SeqCst);
            HOOK_ID.with(|id_cell|id_cell.set(id));
            id
        }
        thread_local! {
            static HOOK_ID: Cell<usize> = Cell::new(0);
        }
        static HOOK_ID_GEN: AtomicUsize = AtomicUsize::new(0);
        lazy_static! {
            static ref HOOKS: Mutex<HashMap<usize, Box<PanicHookFn>>> = {
                let old_hook = panic::take_hook();
                panic::set_hook(Box::new(move |panic_info| {
                    old_hook(panic_info);
                    let id = HOOK_ID.with(|id| id.get());
                    if let Some(hook) = HOOKS.lock().unwrap().get(&id) {
                        hook(panic_info);
                    }
                }));

                Mutex::new(HashMap::new())
            };
        }
    }

    fn backtrace_from_panic_hook(inner: impl FnOnce() + UnwindSafe) -> String {
        let result_cell = Arc::new(Mutex::new(None));
        let result_cell2 = result_cell.clone();
        let hook = Box::new(move |_panic_info: &PanicInfo| {
            let out = crate::create_backtrace();
            *result_cell.lock().unwrap() = Some(out);
        });

        with_panic_hook(hook, inner);

        let mut cell = result_cell2.lock().unwrap();
        cell.take().unwrap()
    }

    #[test]
    fn backtrace_in_panic_hook() {
        let bt = backtrace_from_panic_hook(|| panic!("test backtrace from panic hook"));
        assert!(bt.trim().len() > 0);
    }

    // Note: This tests might brake/start failing with **non braking changes** in rustc and/or std
    #[test]
    fn instable_backtrace_in_panic_hook() {
        let bt = backtrace_from_panic_hook(|| panic!("test backtrace from panic hook"));
        let expected_bt = r#"
            0: std::panic::catch_unwind::{@}
                at /rustc/{@}/src/libstd/panic.rs:{@}
            1: backtrace_string::tests::with_panic_hook::{@}
                at src/lib.rs:{@}
            2: backtrace_string::tests::backtrace_from_panic_hook::{@}
                at src/lib.rs:{@}
            3: backtrace_string::tests::instable_backtrace_in_panic_hook::{@}
                at src/lib.rs:{@}
            4: backtrace_string::tests::instable_backtrace_in_panic_hook::{{closure}}::{@}
                at src/lib.rs:{@}
            5: core::ops::function::FnOnce::call_once::{@}
                at /rustc/{@}/src/libcore/ops/function.rs:{@}
            6: <alloc::boxed::Box<F> as core::ops::function::FnOnce<A>>::call_once::{@}
                at /rustc/{@}/src/liballoc/boxed.rs:{@}
            7: __rust_maybe_catch_panic
                at src/libpanic_unwind/lib.rs:{@}
            8: std::panicking::try::{@}
                at /rustc/{@}/src/libstd/panicking.rs:{@}
               std::panic::catch_unwind::{@}
                at /rustc/{@}/src/libstd/panic.rs:{@}
               test::run_test::run_test_inner::{{closure}}::{@}
                at src/libtest/lib.rs:{@}
        "#;
        fuzzy_stacktrace_eq(expected_bt, bt);
    }

    #[test]
    fn backtrace_outside_of_panic_hook() {
        let bt = crate::create_backtrace();
        assert!(bt.trim().len() > 0);
    }

    #[test]
    fn instable_backtrace_outside_of_panic_hook() {
        let bt = crate::create_backtrace();
        let expected_bt = r#"
            0: backtrace_string::create_backtrace::{@}
                at src/lib.rs:{@}
            1: backtrace_string::tests::instable_backtrace_outside_of_panic_hook::{@}
                at src/lib.rs:{@}
            2: backtrace_string::tests::instable_backtrace_outside_of_panic_hook::{{closure}}::{@}
                at src/lib.rs:{@}
            3: core::ops::function::FnOnce::call_once::{@}
                at /rustc/{@}/src/libcore/ops/function.rs:{@}
            4: <alloc::boxed::Box<F> as core::ops::function::FnOnce<A>>::call_once::{@}
                at /rustc/{@}/src/liballoc/boxed.rs:{@}
            5: __rust_maybe_catch_panic
                at src/libpanic_unwind/lib.rs:{@}
            6: std::panicking::try::{@}
                at /rustc/{@}/src/libstd/panicking.rs:{@}
               std::panic::catch_unwind::{@}
                at /rustc/{@}/src/libstd/panic.rs:{@}
               test::run_test::run_test_inner::{{closure}}::{@}
                at src/libtest/lib.rs:{@}
        "#;

        fuzzy_stacktrace_eq(expected_bt, bt);
    }

    fn fuzzy_stacktrace_eq(expected: &'static str, got: String) {
        let mut exp_lines = expected.trim().lines()
            .map(|line| line.trim());
        let mut got_lines = got.trim().lines()
            .map(|line| line.trim());

        loop {
            let (exp, mut got) = match (exp_lines.next(), got_lines.next()) {
                (Some(exp), Some(got)) => (exp, got),
                (Some(exp), None) => panic!("expected backtrace has additional lines, starting with {:?}", exp),
                (None, Some(got)) => panic!("created backtrace has additional lines, starting with {:?}", got),
                (None, None) => break
            };

            for part in exp.split("{@}") {
                if !got.starts_with(part) {
                    panic!("Mismatch {:?} should start with {:?}", got, part);
                }

                got = &got[part.len()..];

                got = got.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
            }
        }
    }

}
