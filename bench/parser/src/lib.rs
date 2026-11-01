//! Benchmark of `sqlparser` compiled to WebAssembly: the parser that the
//! Worker of the Pivoteur runs on the expressions of its virtual columns and
//! measures (`docs/PERFORMANCES.md`, chapter 13). The module parses one
//! expression at a time with the generic dialect of `sqlparser`, the one
//! nearest to ANSI SQL, where the dialect of the Pivoteur starts.
//!
//! The functions are exported without `wasm-bindgen`: JavaScript reserves
//! memory in the module, writes the UTF-8 text of an expression into it, then
//! asks for its parse (`../run.mjs`).

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::slice;

use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::Token;

/// Layout of `size` bytes of memory reserved for JavaScript.
fn layout(size: usize) -> Layout {
    Layout::from_size_align(size.max(1), 1).expect("size below the memory of the module")
}

/// Reserves `size` bytes of memory for JavaScript, and returns their address.
#[unsafe(no_mangle)]
pub extern "C" fn reserve(size: usize) -> *mut u8 {
    let layout = layout(size);
    // SAFETY: the layout has a size above zero.
    let pointer = unsafe { alloc(layout) };
    if pointer.is_null() {
        handle_alloc_error(layout);
    }
    pointer
}

/// Frees memory reserved for JavaScript.
///
/// # Safety
///
/// `pointer` comes from `reserve(size)`, and is freed only once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn release(pointer: *mut u8, size: usize) {
    // SAFETY: given by the caller.
    unsafe { dealloc(pointer, layout(size)) }
}

/// Parses the `length` bytes of UTF-8 text at `text`: 1 when they form one
/// whole expression, 0 otherwise (invalid UTF-8, syntax error, or tokens left
/// after the expression).
///
/// # Safety
///
/// `text` comes from `reserve`, holds `length` bytes, and stays untouched
/// during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn parse(text: *const u8, length: usize) -> u32 {
    // SAFETY: given by the caller.
    let bytes = unsafe { slice::from_raw_parts(text, length) };
    let Ok(sql) = std::str::from_utf8(bytes) else {
        return 0;
    };
    let parsed = Parser::new(&GenericDialect {})
        .try_with_sql(sql)
        .and_then(|mut parser| {
            let expression = parser.parse_expr()?;
            parser.expect_token(&Token::EOF)?;
            Ok(expression)
        });
    u32::from(parsed.is_ok())
}
