use super::{Branch, PatternTree, RangeBranch, VariantBranch, WildcardKeeper};
use crate::{Constructor, Constructors, Pattern, SumtypeConstructor};
use std::ops::RangeInclusive;

#[derive(Debug)]
struct Progress<'a, C: Constructors> {
    final_: &'a mut Versions<C>,
    params: Option<ParamProgress<C>>,
}

#[derive(Clone, Debug)]
struct ParamProgress<C: Constructors> {
    buf: Vec<Pattern<C>>,
    constr: Constructor<C>,
    remaining: usize,
    parent: Box<Option<Self>>,
}

impl<C: Constructors> PatternTree<C> {
    pub fn generate_missing_patterns(&self) -> Vec<Pattern<C>> {
        let mut versions = Versions(vec![]);
        self.get_missing(Progress::new(&mut versions));
        versions.0
    }

    fn get_missing(&self, mut prog: Progress<'_, C>) {
        match self {
            &PatternTree::SignedInteger { bitsize, ref branches } => {
                let has_missing_ranges = |branches: &[RangeBranch<C, i128>]| -> bool {
                    // lol wtf, we need to fix this.
                    let together = ((branches
                        .iter()
                        .map(|Branch { data: range, .. }| i128::abs(*range.start() - *range.end()))
                        .sum::<i128>()
                        + (branches.len() as i128))
                        / 2)
                        - 1;
                    let max = super::merge::signed_max(bitsize as u32);
                    together != max as i128
                };

                if has_missing_ranges(branches) {
                    return prog.clone().rest_is_missing();
                }

                // sub-missing of included ranges
                for Branch { data: range, con } in branches {
                    let pattern =
                        Pattern::new(Constructor::SignedInteger { range: range.clone(), bitsize });
                    let prog = prog.clone().include(pattern);
                    con.get_missing(prog)
                }
            }
            PatternTree::Variant(type_, branches) => {
                let max = type_.max();
                for tag in 0..=max {
                    match branches.iter().find(|branch| branch.data == tag) {
                        Some(branch) => {
                            let params = type_.params_for(branch.data);
                            branch.con.get_missing(prog.clone().new_params(
                                Constructor::Variant { type_: type_.clone(), tag },
                                params,
                            ));
                        }
                        None => prog.clone().rest_is_missing(),
                    }
                }
            }
            PatternTree::Constant(constr, wc, branches) => {
                if !prog.clone().include_wildcard(wc) {
                    prog.include_branches(
                        branches,
                        |_| Constructor::Constant(constr.clone()),
                        |params| *params,
                    );
                }
            }
            PatternTree::Infinite(wc, branches) => {
                if !prog.clone().include_wildcard(wc) {
                    prog.include_branches(
                        branches,
                        |constr| Constructor::Infinite(constr.clone()),
                        |_| 0,
                    );
                }
            }
            PatternTree::None => {
                assert!(matches!(prog.params, None));
            }
            other => todo!("{:?}", other),
        }
    }
}

#[derive(Debug)]
struct Versions<C: Constructors>(Vec<Pattern<C>>);

impl<'a, C: Constructors> Progress<'a, C> {
    fn new(final_: &'a mut Versions<C>) -> Self {
        Self { final_, params: None }
    }

    fn into_parent(self, constr: Constructor<C>, params: usize) -> Progress<'a, C> {
        Progress {
            final_: self.final_,
            params: Some(ParamProgress::new(constr, params, self.params.clone())),
        }
    }

    fn clone(&mut self) -> Progress<'_, C> {
        Progress {
            final_: self.final_,
            params: self.params.clone(),
        }
    }

    fn include(self, pattern: Pattern<C>) -> Progress<'a, C> {
        match self.params {
            None => Progress::new(self.final_),
            Some(mut pprog) => {
                pprog.remaining -= 1;
                pprog.buf.push(pattern);
                if pprog.remaining == 0 {
                    let constructed = Pattern::new(pprog.constr).with_params(pprog.buf);
                    Progress {
                        final_: self.final_,
                        params: *pprog.parent,
                    }
                    .include(constructed)
                } else {
                    Progress {
                        final_: self.final_,
                        params: Some(pprog),
                    }
                }
            }
        }
    }

    fn include_branches<A>(
        &mut self,
        branches: &[Branch<C, A>],
        to_constr: impl Fn(&A) -> Constructor<C>,
        params_of: impl Fn(&A) -> usize,
    ) {
        for Branch { data, con } in branches {
            con.get_missing(self.clone().new_params(to_constr(data), params_of(data)));
        }
    }

    fn include_wildcard(self, wc: &WildcardKeeper<C>) -> bool {
        let pattern = Pattern::wildcard(C::Wildcard::default());
        match wc.con.as_deref() {
            None => {
                self.include(pattern).rest_is_missing();
                false
            }
            Some(con) => {
                let prog = self.include(pattern);
                con.get_missing(prog);
                true
            }
        }
    }

    fn rest_is_missing(self) {
        let wc = Pattern::wildcard(C::Wildcard::default());

        match self.params {
            Some(mut pprog) => {
                for _ in 0..pprog.remaining {
                    pprog.buf.push(wc.clone());
                }
                let constructed = Pattern::new(pprog.constr).with_params(pprog.buf);

                match *pprog.parent {
                    None => self.final_.0.push(constructed),
                    Some(parent) => todo!("recurse"),
                }
            }
            None => {
                self.final_.0.push(wc);
            }
        }
    }

    fn new_params(self, constr: Constructor<C>, params: usize) -> Self {
        if params == 0 {
            self.include(Pattern::new(constr))
        } else {
            self.into_parent(constr, params)
        }
    }
}

impl<C: Constructors> ParamProgress<C> {
    fn new(constr: Constructor<C>, params: usize, parent: Option<ParamProgress<C>>) -> Self {
        Self {
            remaining: params,
            buf: Vec::with_capacity(params),
            constr,
            parent: Box::new(parent),
        }
    }
}
