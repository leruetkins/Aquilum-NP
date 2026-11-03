mod colors;
mod platform_color;

pub use colors::*;
pub use platform_color::*;

mod classify;
mod code_scan;

pub use classify::*;
pub use code_scan::*;
mod grammar;
pub use grammar::*;
mod inline;
mod syntax_spans;

pub use inline::*;
pub use syntax_spans::*;
mod visual;
pub use visual::*;
mod table_rows;
pub use table_rows::*;
mod list;
mod quote;

pub use list::*;
pub use quote::*;
mod paint;
pub use paint::*;

#[cfg(test)]
mod tests;



