use super::{Constructor, Constructors, FlatPatterns, Pattern, SumtypeConstructor};
use itertools::Itertools;
use std::fmt;
use std::ops::RangeInclusive;

pub(crate) type Params = usize;

mod merge;
use merge::Merge;
mod missing;

#[derive(Clone, Debug)]
pub enum PatternTree<C: Constructors> {
    SignedInteger {
        bitsize: u8,
        branches: Vec<(RangeInclusive<i128>, PatternTree<C>)>,
    },
    UnsignedInteger {
        bitsize: u8,
        branches: Vec<(RangeInclusive<u128>, PatternTree<C>)>,
    },

    Variant(C::SumType, Vec<VariantBranch<C>>),
    Constant(C::Constant, WildcardKeeper<C>, Vec<ConstantBranch<C>>),
    Infinite(WildcardKeeper<C>, Vec<InfiniteBranch<C>>),

    // UnknownWildcard(C::Wildcard, Box<Self>),
    UnknownWildcard(WildcardKeeper<C>),
    None,
}

pub(crate) type VariantBranch<C> = (u64, PatternTree<C>);
pub(crate) type ConstantBranch<C> = (Params, PatternTree<C>);
pub(crate) type InfiniteBranch<C> = (<C as Constructors>::Infinite, PatternTree<C>);
pub(crate) type RangeBranch<C, N> = (RangeInclusive<N>, PatternTree<C>);

// for some constructors like infinite we hold on to the wildcard variants so we can re-merge
// them into any additional variants we create afterwards.
//
// TODO: this feels hacky and unecesarry D:
#[derive(Clone, Debug)]
pub struct WildcardKeeper<C: Constructors> {
    buf: Vec<(C::Wildcard, FlatPatterns<C>)>,
    con: Option<Box<PatternTree<C>>>,
}

impl<C: Constructors> WildcardKeeper<C> {
    fn new() -> Self {
        Self { buf: vec![], con: None }
    }

    fn init(wc: C::Wildcard, con: &mut FlatPatterns<C>) -> Self {
        let mut keeper = Self::new();
        keeper.buf.push((wc, con.clone()));
        keeper.con = Some(Box::new(con.drain_to_patterntree()));
        keeper
    }

    fn with_wildcard(&mut self, wc: C::Wildcard, mut con: FlatPatterns<C>) -> IsReachable {
        self.buf.push((wc, con.clone()));

        match &mut self.con {
            Some(existing) => con.merge_with(existing),
            a @ None => {
                *a = Some(Box::new(con.drain_to_patterntree()));
                IsReachable(true)
            }
        }
    }

    fn with_branch(
        &self,
        branch_con: Option<&mut PatternTree<C>>,
        mut src: FlatPatterns<C>,
    ) -> (IsReachable, Option<PatternTree<C>>) {
        match branch_con {
            Some(econ) => {
                let reachable_via_version = src.clone().merge_with(econ);

                match self.con.as_deref().cloned() {
                    Some(PatternTree::None) => (IsReachable(false), None),
                    Some(_) | None => (reachable_via_version, None),
                }
            }
            None => {
                let mut continuation = src.drain_to_patterntree();
                let is_reachable = self.include_into_version(&mut continuation);
                // self.branches.push((params, continuation));
                (is_reachable, Some(continuation))
            }
        }
    }

    fn include_into_version(&self, given_con: &mut PatternTree<C>) -> IsReachable {
        if self.buf.is_empty() {
            return IsReachable(true);
        }

        self.buf
            .iter()
            .fold(IsReachable(true), |is_r, (_, previous_wc)| {
                is_r & previous_wc.clone().merge_with(given_con)
            })
    }
}

impl<C: Constructors> PatternTree<C> {
    fn is_end(&self) -> bool {
        matches!(self, Self::None)
    }

    fn on_continuation(&mut self, mut params: usize, f: &mut impl Fn(&mut Self)) {
        if params == 0 {
            f(self)
        } else {
            use PatternTree::*;

            params -= 1;

            match self {
                Variant(type_, branches) => branches
                    .iter_mut()
                    .for_each(|(tag, con)| con.on_continuation(params + type_.params_for(*tag), f)),
                SignedInteger { branches, .. } => branches
                    .iter_mut()
                    .for_each(|(_, con)| con.on_continuation(params, f)),
                UnsignedInteger { branches, .. } => branches
                    .iter_mut()
                    .for_each(|(_, con)| con.on_continuation(params, f)),
                Constant(_, wc, branches) => {
                    branches
                        .iter_mut()
                        .for_each(|(p, con)| con.on_continuation(params + *p, f));

                    if let Some(con) = &mut wc.con {
                        con.on_continuation(params, f);
                    }
                }
                Infinite(wc, branches) => {
                    branches
                        .iter_mut()
                        .for_each(|(_, con)| con.on_continuation(params, f));

                    if let Some(con) = &mut wc.con {
                        con.on_continuation(params, f);
                    }
                }
                UnknownWildcard(keeper) => {
                    if let Some(con) = keeper.con.as_deref_mut() {
                        con.on_continuation(params, f)
                    }
                }
                None => panic!("FlatPatterns ended unexpectedly"),
            }
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct IsReachable(pub bool);

impl std::ops::BitOr for IsReachable {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        IsReachable(self.0 | other.0)
    }
}
impl std::ops::BitAnd for IsReachable {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        IsReachable(self.0 & other.0)
    }
}
impl std::ops::BitOrAssign for IsReachable {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}
impl std::ops::BitAndAssign for IsReachable {
    fn bitand_assign(&mut self, other: Self) {
        self.0 &= other.0;
    }
}

impl<C: Constructors> PatternTree<C> {
    pub fn from_pattern(p: &Pattern<C>) -> Self {
        p.flatten().drain_to_patterntree()
    }

    pub fn include_pattern(&mut self, p: &Pattern<C>) -> IsReachable {
        p.flatten().merge_with(self)
    }

    pub fn is_exhaustive(&self) -> bool {
        self.clone()
            .include_pattern(&Pattern::wildcard(C::Wildcard::default()))
            == IsReachable(false)
    }
}

impl<C: Constructors> Constructor<C> {
    fn into_patterntree(self, params: usize, src: &mut FlatPatterns<C>) -> PatternTree<C> {
        match self {
            Self::Variant { type_, tag } => {
                PatternTree::Variant(type_, vec![(tag, src.drain_to_patterntree())])
            }

            Self::SignedInteger { range, bitsize } => PatternTree::SignedInteger {
                bitsize,
                branches: vec![(range, src.drain_to_patterntree())],
            },
            Self::UnsignedInteger { range, bitsize } => PatternTree::UnsignedInteger {
                bitsize,
                branches: vec![(range, src.drain_to_patterntree())],
            },
            Self::Constant(constr) => PatternTree::Constant(
                constr,
                WildcardKeeper::new(),
                vec![(params, src.drain_to_patterntree())],
            ),
            Self::Infinite(constr) => PatternTree::Infinite(
                WildcardKeeper::new(),
                vec![(constr, src.drain_to_patterntree())],
            ),
            Self::Wildcard(wc) => PatternTree::UnknownWildcard(WildcardKeeper::init(wc, src)),
        }
    }
}

impl<C: Constructors> FlatPatterns<C> {
    fn clone_to_padded(&self, padding: usize) -> FlatPatterns<C> {
        let mut clone = self.clone();
        for _ in 0..padding {
            clone.push_front((Constructor::Wildcard(C::Wildcard::default()), 0))
        }
        clone
    }

    fn merge_with(self, tree: &mut PatternTree<C>) -> IsReachable {
        Merge::new(self, tree).run()
    }

    // TODO: we can optimize this a lot by working with 'self' instead of '&mut self'
    pub fn drain_to_patterntree(&mut self) -> PatternTree<C> {
        match self.pop_front() {
            Some((constr, params)) => constr.into_patterntree(params, self),
            None => PatternTree::None,
        }
    }
}

impl<C: Constructors> PatternTree<C> {
    fn fmt_cont(&self) -> String {
        if self.is_end() {
            return String::new();
        }
        format!(":\n  {}", self.to_string().lines().format("\n  "))
    }
}

impl<C: Constructors> fmt::Display for PatternTree<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PatternTree::SignedInteger { branches, .. } => branches
                .iter()
                .try_for_each(|(range, continuation)| writeln!(f, "{:?}{}", range, continuation)),
            PatternTree::UnsignedInteger { branches, .. } => branches
                .iter()
                .try_for_each(|(range, continuation)| writeln!(f, "{:?}{}", range, continuation)),
            PatternTree::Variant(constr, branches) => {
                branches.iter().try_for_each(|(tag, continuation)| {
                    writeln!(f, "{:?}[{}]{}", constr, tag, continuation.fmt_cont())
                })
            }
            PatternTree::Infinite(wildcard, branches) => branches
                .iter()
                .try_for_each(|(constr, continuation)| {
                    writeln!(f, "{:?}{}", constr, continuation.fmt_cont())
                })
                .and_then(|_| {
                    wildcard
                        .buf
                        .iter()
                        .try_for_each(|wc| writeln!(f, "{:?}:{:?}", wc.0, wc.1))
                }),
            PatternTree::Constant(constr, wildcard, branches) => branches
                .iter()
                .try_for_each(|(_, continuation)| {
                    writeln!(f, "{:?}{}", constr, continuation.fmt_cont())
                })
                .and_then(|_| {
                    wildcard
                        .buf
                        .iter()
                        .try_for_each(|wc| writeln!(f, "{:?}:{:?}", wc.0, wc.1))
                }),
            PatternTree::UnknownWildcard(keeper) => writeln!(
                f,
                "{:?}{}",
                &keeper.buf[0].0,
                keeper
                    .con
                    .as_deref()
                    .unwrap_or(&PatternTree::None)
                    .fmt_cont()
            ),
            PatternTree::None => Ok(()),
        }
    }
}
