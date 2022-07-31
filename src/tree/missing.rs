use super::PatternTree;
use crate::{Constructor, Constructors, Pattern, SumtypeConstructor};

impl<C: Constructors> PatternTree<C> {
    pub fn generate_missing_patterns(&self) -> Vec<Pattern<C>> {
        let mut gen = MissingGenerator { remaining: 1, tree: self };
        let missing = gen.next();
        assert!(matches!(gen.tree, PatternTree::None));
        missing.0
    }
}

// type Wrap<C> = dyn Fn(Pattern<C>, &mut Vec<Pattern<C>>);

struct MissingGenerator<'a, C: Constructors> {
    tree: &'a PatternTree<C>,
    remaining: usize,
}

struct Versions<C: Constructors>(Vec<Pattern<C>>);
struct Params<C: Constructors>(Vec<Pattern<C>>);

impl<'a, C: Constructors> MissingGenerator<'a, C> {
    fn next(&mut self) -> Versions<C> {
        match self.tree {
            PatternTree::None => {
                // self.finalize(wrap)
                todo!();
            }
            PatternTree::Variant(type_, branches) => todo!(),
            _ => todo!(),
        }
    }

    fn params(&mut self, params: usize) -> Params<C> {
        todo!();
    }
}
