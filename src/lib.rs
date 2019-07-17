//! `backtrace-string` generates a backtrace as a human readable string.
//!
//! This library uses the [`backtrace` crate](https://crates.io/crates/backtrac)
//! to generate a backtrace and then converts it to a human readable string
//! by demangleing names, doing some formating etc.
//!
//!

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