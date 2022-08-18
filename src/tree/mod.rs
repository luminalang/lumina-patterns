use super::{
    ConstantConstructor, Constructor, Constructors, FlatPatterns, Pattern, SumtypeConstructor,
};
use itertools::Itertools;
use std::fmt;
use std::ops::RangeInclusive;

pub(crate) type Params = usize;

pub(crate) mod merge;
use merge::Merge;
mod missing;

#[derive(Clone, Debug)]
pub enum PatternTree<C: Constructors> {
    SignedInteger {
        bitsize: u8,
        branches: Vec<RangeBranch<C, i128>>,
    },
    UnsignedInteger {
        bitsize: u8,
        branches: Vec<RangeBranch<C, u128>>,
    },

    Variant(C::SumType, Vec<VariantBranch<C>>),
    Lengthed(C::Lengthed, WildcardKeeper<C>, Vec<LengthedBranch<C>>),
    Constant(C::Constant, Box<Self>),
    Infinite(WildcardKeeper<C>, Vec<InfiniteBranch<C>>),

    UnknownWildcard(WildcardKeeper<C>),
    None,
}

#[derive(Clone, Debug)]
pub struct Branch<C: Constructors, A> {
    pub(crate) con: PatternTree<C>,
    pub(crate) data: A,
}

trait Branches<C: Constructors, A> {
    fn get_matching(&mut self, a: &A) -> Option<&mut PatternTree<C>>;
    fn on_continuation(
        &mut self,
        params: usize,
        params_of: impl Fn(&A) -> usize,
        f: &mut impl FnMut(&mut PatternTree<C>),
    );
}

impl<C: Constructors, A: PartialEq> Branches<C, A> for Vec<Branch<C, A>> {
    fn get_matching(&mut self, a: &A) -> Option<&mut PatternTree<C>> {
        self.iter_mut()
            .find_map(|Branch { data, con }| if data == a { Some(con) } else { None })
    }

    fn on_continuation(
        &mut self,
        params: usize,
        params_of: impl Fn(&A) -> usize,
        f: &mut impl FnMut(&mut PatternTree<C>),
    ) {
        self.iter_mut()
            .for_each(|Branch { data, con }| con.on_continuation(params + params_of(data), f))
    }
}

pub(crate) type VariantBranch<C> = Branch<C, u64>;
pub(crate) type LengthedBranch<C> = Branch<C, Params>;
pub(crate) type InfiniteBranch<C> = Branch<C, <C as Constructors>::Infinite>;
pub(crate) type RangeBranch<C, N> = Branch<C, RangeInclusive<N>>;

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
                let is_reachable = self.merge_into_version(&mut continuation);
                (is_reachable, Some(continuation))
            }
        }
    }

    // when inserting as a new version we need to first merge any previous wildcards
    // into that version to make it up-to-date.
    fn merge_into_version(&self, given_con: &mut PatternTree<C>) -> IsReachable {
        if self.buf.is_empty() {
            return IsReachable(true);
        }

        if matches!(self.con.as_deref(), Some(&PatternTree::None)) {
            return IsReachable(false);
        }

        let any_previous_wc_con_contains_version_con = self
            .buf
            .iter()
            .any(|(_, previous_wc)| previous_wc.clone().merge_with(given_con).0);

        IsReachable(!any_previous_wc_con_contains_version_con)
    }
}

impl<C: Constructors> PatternTree<C> {
    fn is_end(&self) -> bool {
        matches!(self, Self::None)
    }

    fn on_continuation(&mut self, mut params: usize, f: &mut impl FnMut(&mut Self)) {
        if params == 0 {
            f(self)
        } else {
            use PatternTree::*;

            params -= 1;

            match self {
                Variant(type_, branches) => {
                    branches.on_continuation(params, |tag| type_.params_for(*tag), f)
                }
                SignedInteger { branches, .. } => branches.on_continuation(params, |_| 0, f),
                UnsignedInteger { branches, .. } => branches.on_continuation(params, |_| 0, f),
                Lengthed(_, wc, branches) => {
                    branches.on_continuation(params, |p| *p, f);

                    if let Some(con) = &mut wc.con {
                        con.on_continuation(params, f);
                    }
                }
                Constant(constr, con) => con.on_continuation(params + constr.len_requirement(), f),
                Infinite(wc, branches) => {
                    branches.on_continuation(params, |_| 0, f);

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
            Self::Variant { type_, tag } => PatternTree::Variant(type_, src.drain_to_branches(tag)),
            Self::SignedInteger { range, bitsize } => PatternTree::SignedInteger {
                bitsize,
                branches: src.drain_to_branches(range),
            },
            Self::UnsignedInteger { range, bitsize } => PatternTree::UnsignedInteger {
                bitsize,
                branches: src.drain_to_branches(range),
            },
            Self::Lenghted(constr) => {
                PatternTree::Lengthed(constr, WildcardKeeper::new(), src.drain_to_branches(params))
            }
            Self::Constant(constr) => {
                PatternTree::Constant(constr, Box::new(src.drain_to_patterntree()))
            }
            Self::Infinite(constr) => {
                PatternTree::Infinite(WildcardKeeper::new(), src.drain_to_branches(constr))
            }
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

    fn drain_to_branches<A>(&mut self, data: A) -> Vec<Branch<C, A>> {
        vec![Branch {
            data,
            con: self.drain_to_patterntree(),
        }]
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

impl<C: Constructors, A: std::fmt::Debug> fmt::Display for Branch<C, A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}{}", self.data, self.con.fmt_cont())
    }
}

fn fmt_branches<C: Constructors, A: std::fmt::Debug>(
    f: &mut fmt::Formatter,
    branches: &[Branch<C, A>],
) -> fmt::Result {
    branches
        .iter()
        .try_for_each(|branch| writeln!(f, "{}", branch))
}

fn fmt_wildcard<C: Constructors>(f: &mut fmt::Formatter, wc: &WildcardKeeper<C>) -> fmt::Result {
    let name = wc
        .buf
        .get(0)
        .map(|(card, _)| card.clone())
        .unwrap_or_else(C::Wildcard::default);
    if let Some(con) = wc.con.as_deref() {
        writeln!(f, "{:?}{}", name, con.fmt_cont())
    } else {
        Ok(())
    }
}

impl<C: Constructors> fmt::Display for PatternTree<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PatternTree::SignedInteger { branches, .. } => fmt_branches(f, branches),
            PatternTree::UnsignedInteger { branches, .. } => fmt_branches(f, branches),
            PatternTree::Variant(constr, branches) => {
                branches.iter().try_for_each(|Branch { data: tag, con }| {
                    writeln!(f, "{:?}[{}]{}", constr, tag, con.fmt_cont())
                })
            }
            PatternTree::Infinite(wc, branches) => {
                fmt_branches(f, branches).and_then(|_| fmt_wildcard(f, wc))
            }
            PatternTree::Lengthed(constr, wc, branches) => branches
                .iter()
                .try_for_each(|Branch { con, .. }| writeln!(f, "{:?}{}", constr, con.fmt_cont()))
                .and_then(|_| fmt_wildcard(f, wc)),
            PatternTree::Constant(constr, con) => write!(f, "{:?}{}", constr, con.fmt_cont()),
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
