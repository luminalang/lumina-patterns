#[cfg(test)]
mod tests;

mod pattern;
use pattern::FlatPatterns;
pub use pattern::{ConstantConstructor, Constructor, Constructors, Pattern, SumtypeConstructor};

mod tree;
pub use tree::{IsReachable, PatternTree};
