use super::*;
use crate::pattern::SumtypeConstructor;
use std::ops::RangeInclusive;

pub(super) struct Merge<'t, C: Constructors> {
    dst: &'t mut PatternTree<C>,
    src: FlatPatterns<C>,
}

impl<'t, C: Constructors> Merge<'t, C> {
    pub fn new(src: FlatPatterns<C>, dst: &'t mut PatternTree<C>) -> Self {
        Self { dst, src }
    }

    pub fn run(mut self) -> IsReachable {
        match self.src.pop_front() {
            None => IsReachable(false),
            Some((constr, params)) => match (constr, self.dst) {
                (Constructor::Variant { type_, tag }, PatternTree::Variant(_, branches)) => {
                    self.src.into_merger(branches).with_variant(&type_, tag)
                }

                (Constructor::Constant(constr), PatternTree::Constant(_, wc, branches)) => self
                    .src
                    .into_merger(branches)
                    .with_constant(constr, wc, params),

                (
                    Constructor::SignedInteger { range, bitsize: bs },
                    PatternTree::SignedInteger { branches, bitsize },
                ) => {
                    assert_eq!(params, 0);
                    assert_eq!(bs, *bitsize, "inconsistent bitsize of range patterns");
                    self.src.into_merger(branches).with_range(range)
                }

                (Constructor::Infinite(constr), PatternTree::Infinite(wc, branches)) => {
                    self.src.into_merger(branches).with_infinite(constr, wc)
                }

                (Constructor::Wildcard(wc), dst) => match dst {
                    PatternTree::SignedInteger { branches, bitsize } => self
                        .src
                        .into_merger(branches)
                        .with_wildcard_integer(wc, *bitsize as u32),
                    PatternTree::Variant(constr, branches) => self
                        .src
                        .into_merger(branches)
                        .with_wildcard_variant(constr, wc),
                    PatternTree::Infinite(wildcard, branches) => self
                        .src
                        .into_merger(branches)
                        .with_wildcard_infinite(wildcard, wc),
                    PatternTree::Constant(_, wildcard, branches) => self
                        .src
                        .into_merger(branches)
                        .with_wildcard_constant(wildcard, wc),
                    _ => todo!(),
                },

                (constr, r @ PatternTree::UnknownWildcard(_)) => {
                    self.src.push_front((constr.clone(), params));
                    take_mut::take(r, |dst| match dst {
                        PatternTree::UnknownWildcard(keeper) => {
                            Self::init_from_wc(constr, &self.src, keeper)
                        }
                        _ => unreachable!(),
                    });
                    self.src.merge_with(r)
                }

                _ => todo!(),
            },
        }
    }

    fn init_from_wc(
        constr: Constructor<C>,
        src: &FlatPatterns<C>,
        mut keeper: WildcardKeeper<C>,
    ) -> PatternTree<C> {
        match constr {
            Constructor::Wildcard(wc) => {
                keeper.buf.push((wc, src.clone()));
                PatternTree::UnknownWildcard(keeper)
            }
            Constructor::Constant(constr) => PatternTree::Constant(constr, keeper, vec![]),
            Constructor::Infinite(_) => PatternTree::Infinite(keeper, vec![]),
            Constructor::Variant { type_, .. } => PatternTree::Variant(type_, vec![]),
            Constructor::SignedInteger { bitsize, .. } => PatternTree::SignedInteger {
                bitsize,
                branches: vec![(min_max_from_bitsize(bitsize as u32), *keeper.con.unwrap())],
            },
            Constructor::UnsignedInteger { .. } => todo!("unsigned integers"),
        }
    }
}

struct Merger<'t, C: Constructors, B> {
    branches: &'t mut Vec<B>,
    ptr: usize,
    src: FlatPatterns<C>,
}

impl<C: Constructors> FlatPatterns<C> {
    fn into_merger<B>(self, branches: &mut Vec<B>) -> Merger<'_, C, B> {
        Merger { src: self, branches, ptr: 0 }
    }
}

impl<'t, C: Constructors> Merger<'t, C, ConstantBranch<C>> {
    fn with_constant(
        self,
        _: C::Constant,
        wc: &mut WildcardKeeper<C>,
        params: Params,
    ) -> IsReachable {
        let (is_reachable, to_push) = wc.with_branch(
            self.branches
                .iter_mut()
                .find(|(eparams, _)| *eparams == params)
                .map(|(_, con)| con),
            self.src,
        );
        if let Some(con) = to_push {
            self.branches.push((params, con));
        }
        is_reachable
    }

    fn with_wildcard_constant(
        self,
        existing: &mut WildcardKeeper<C>,
        wc: C::Wildcard,
    ) -> IsReachable {
        for (params, econ) in self.branches.iter_mut() {
            econ.on_continuation(*params, &mut |con| {
                self.src.clone().merge_with(con);
            });
        }
        existing.with_wildcard(wc, self.src)
    }
}

impl<'t, C: Constructors> Merger<'t, C, VariantBranch<C>> {
    fn with_variant(mut self, _: &C::SumType, tag: u64) -> IsReachable {
        match self.branches.iter_mut().find(|(etag, _)| *etag == tag) {
            Some((_, econ)) => self.src.merge_with(econ),
            None => {
                self.branches.push((tag, self.src.drain_to_patterntree()));
                IsReachable(true)
            }
        }
    }

    fn with_wildcard_variant(self, constr: &C::SumType, _: C::Wildcard) -> IsReachable {
        let mut is_reachable = IsReachable(false);

        for tag in 0..=constr.max() {
            let params = constr.params_for(tag);

            match self.branches.iter_mut().find(|(etag, _)| *etag == tag) {
                None => {
                    let con = self.src.clone_to_padded(params).drain_to_patterntree();
                    self.branches.push((tag, con));
                    is_reachable = IsReachable(true);
                }
                Some((_, econ)) => {
                    is_reachable |= self.src.clone_to_padded(params).merge_with(econ);
                }
            }
        }

        is_reachable
    }
}

impl<'t, C: Constructors> Merger<'t, C, InfiniteBranch<C>> {
    fn with_infinite(self, constr: C::Infinite, wc: &WildcardKeeper<C>) -> IsReachable {
        let (is_reachable, to_push) = wc.with_branch(
            self.branches
                .iter_mut()
                .find(|(econstr, _)| *econstr == constr)
                .map(|(_, con)| con),
            self.src,
        );
        if let Some(con) = to_push {
            self.branches.push((constr, con));
        }
        is_reachable
    }

    fn with_wildcard_infinite(
        self,
        existing: &mut WildcardKeeper<C>,
        wc: C::Wildcard,
    ) -> IsReachable {
        for (_, econ) in self.branches.iter_mut() {
            let params = 0; // TODO: we're gonna re-add support for this, there's no reason not to
            econ.on_continuation(params, &mut |con| {
                self.src.clone().merge_with(con);
            });
        }
        existing.with_wildcard(wc, self.src)
    }
}

fn min_max_from_bitsize(bitsize: u32) -> RangeInclusive<i128> {
    let max = max_from_bitsize(bitsize);
    (-max)..=max
}

fn max_from_bitsize(bitsize: u32) -> i128 {
    2i128.pow(bitsize - 1) - 1
}

impl<'t, C: Constructors> Merger<'t, C, RangeBranch<C, i128>> {
    fn with_range(mut self, range: RangeInclusive<i128>) -> IsReachable {
        if self.ptr >= self.branches.len() {
            self.branches.push((range, self.src.drain_to_patterntree()));
            return IsReachable(true);
        }

        let (erange, econ) = &mut self.branches[self.ptr];
        let mut e_start = *erange.start();
        let e_end = *erange.end();
        let start = *range.start();
        let end = *range.end();

        if (start < e_start && end < e_start) || (start > e_end) {
            return self.try_next(range);
        }

        if range == *erange {
            return self.src.merge_with(econ);
        }

        let start_is_inside = start >= e_start;
        let end_is_inside = end <= e_end;

        let mut is_reachable = IsReachable(false);

        if start_is_inside {
            if e_start != start {
                let excluded_left_side = e_start..=start - 1;
                *erange = start..=e_end;
                e_start = start;
                let econ = econ.clone();
                self.branches.push((excluded_left_side, econ));
            }
        } else {
            let extra_left_side = start..=e_start - 1;
            is_reachable |= self.additional(extra_left_side);
        }

        if end_is_inside {
            if e_end != end {
                let excluded_right_side = end + 1..=e_end;
                let (erange, econ) = &mut self.branches[self.ptr];
                let econ = econ.clone();
                *erange = e_start..=end;
                self.branches.push((excluded_right_side, econ));
            }
        } else {
            let extra_right_side = e_end + 1..=end;
            is_reachable |= self.additional(extra_right_side);
        }

        if start_is_inside || end_is_inside {
            let (_, econ) = &mut self.branches[self.ptr];
            is_reachable |= self.src.merge_with(econ);
        }

        is_reachable
    }

    fn with_wildcard_integer(self, _: C::Wildcard, bitsize: u32) -> IsReachable {
        self.with_range(min_max_from_bitsize(bitsize))
    }

    fn additional(&mut self, range: RangeInclusive<i128>) -> IsReachable {
        let mut this = self.src.clone().into_merger(self.branches);
        this.ptr += 1;
        this.with_range(range)
    }

    fn try_next(mut self, range: RangeInclusive<i128>) -> IsReachable {
        self.ptr += 1;
        self.with_range(range)
    }
}
