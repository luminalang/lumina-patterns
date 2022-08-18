use super::{
    ConstantConstructor, Constructor, Constructors, IsReachable, Pattern, PatternTree,
    SumtypeConstructor,
};
use std::ops::RangeInclusive;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Tuple(usize);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Infinite {
    String(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SumType(&'static str, usize);

#[derive(Clone, Debug, PartialEq, Eq)]
struct MyConstructors;

impl Constructors for MyConstructors {
    type Constant = Tuple;
    type Lengthed = ();
    type SumType = SumType;
    type Infinite = Infinite;
    type Wildcard = Wildcard;
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct Wildcard(&'static str);

impl Default for Wildcard {
    fn default() -> Self {
        Wildcard("_")
    }
}

impl SumtypeConstructor for SumType {
    fn max(&self) -> u64 {
        match self.0 {
            "option" => 1,
            _ => panic!("type not found: {}", self.0),
        }
    }

    fn params_for(&self, tag: u64) -> usize {
        match (self.0, tag) {
            ("option", 0) => 1, // just takes 1 params
            ("option", 1) => 0, // none takes 0 params
            _ => panic!("type not found: {}", self.0),
        }
    }
}

impl ConstantConstructor for Tuple {
    fn len_requirement(&self) -> usize {
        self.0
    }
}

fn int(range: RangeInclusive<i64>) -> Pattern<MyConstructors> {
    Pattern::new(Constructor::SignedInteger {
        bitsize: 64,
        range: *range.start() as i128..=*range.end() as i128,
    })
}

fn variant(
    variant: u64,
    max: usize,
    tname: &'static str,
    params: Vec<Pattern<MyConstructors>>,
) -> Pattern<MyConstructors> {
    Pattern::new(Constructor::Variant {
        type_: SumType(tname, max),
        tag: variant,
    })
    .with_params(params)
}

fn wildcard(wc: &'static str) -> Pattern<MyConstructors> {
    Pattern::wildcard(Wildcard(wc))
}

fn just(inner: Pattern<MyConstructors>) -> Pattern<MyConstructors> {
    variant(0, 1, "option", vec![inner])
}
fn none() -> Pattern<MyConstructors> {
    variant(1, 1, "option", vec![])
}

fn tuple<const N: usize>(params: [Pattern<MyConstructors>; N]) -> Pattern<MyConstructors> {
    Pattern::new(Constructor::Constant(Tuple(params.len()))).with_params(params.to_vec())
}

fn string(text: &'static str) -> Pattern<MyConstructors> {
    Pattern::new(Constructor::Infinite(Infinite::String(text)))
}

macro_rules! assert_reach {
    ($tree:ident, $pat:expr, $exp:expr) => {
        println!(" ** inserting {:?}\n", &$pat);
        let is_reachable = $tree.include_pattern(&$pat);
        println!(" :: resulting tree:\n{}", &$tree);
        assert_eq!(is_reachable, $exp);
    };
}

#[test]
fn direct_numbers() {
    let mut tree = PatternTree::from_pattern(&int(0..=0));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, int(0..=1), IsReachable(true));
    assert_reach!(tree, int(0..=1), IsReachable(false));

    let missing = tree.generate_missing_patterns();
    assert!(!missing.is_empty());
}

#[test]
fn maybe_numbers() {
    let mut tree = PatternTree::from_pattern(&just(int(0..=0)));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, just(int(0..=0)), IsReachable(false));
    assert_reach!(tree, none(), IsReachable(true));
    assert_reach!(tree, none(), IsReachable(false));
    assert!(!tree.generate_missing_patterns().is_empty());
}

#[test]
fn maybe_overlapping_numbers() {
    let mut tree = PatternTree::from_pattern(&just(int(0..=5)));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, just(int(2..=8)), IsReachable(true));
    assert_reach!(tree, just(int(1..=7)), IsReachable(false));
    assert_reach!(tree, none(), IsReachable(true));
    assert!(!tree.generate_missing_patterns().is_empty());
}

#[test]
fn weirdness() {
    let mut tree = PatternTree::from_pattern(&int(3..=5));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, int(2..=7), IsReachable(true));
    assert_reach!(tree, int(1..=3), IsReachable(true));
    assert_reach!(tree, int(0..=9), IsReachable(true));
    assert!(!tree.generate_missing_patterns().is_empty());
}

#[test]
fn sequential_numbers() {
    let mut tree = PatternTree::from_pattern(&tuple([int(0..=5), int(2..=3)]));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, tuple([int(2..=8), int(2..=3)]), IsReachable(true));
    assert_reach!(tree, tuple([int(1..=7), int(2..=3)]), IsReachable(false));
    assert!(!tree.generate_missing_patterns().is_empty());
}

#[test]
fn strings() {
    let mut tree = PatternTree::from_pattern(&string("this"));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, string("that"), IsReachable(true));
    assert_reach!(tree, string("thisa"), IsReachable(true));
    assert_reach!(tree, string("this"), IsReachable(false));
    assert!(!tree.generate_missing_patterns().is_empty());
}

#[test]
fn wildcard_no_params() {
    let mut tree = PatternTree::from_pattern(&int(2..=6));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, wildcard("n"), IsReachable(true));
    assert_reach!(tree, int(9..=10), IsReachable(false));
    assert!(dbg!(tree.is_exhaustive()) && dbg!(tree.generate_missing_patterns()).is_empty());
}

#[test]
fn wildcard_can_be_unreachable() {
    let mut tree = PatternTree::from_pattern(&int(i64::MIN..=i64::MAX));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, wildcard("n"), IsReachable(false));
    assert!(tree.is_exhaustive() && dbg!(tree.generate_missing_patterns()).is_empty());

    let mut tree = PatternTree::from_pattern(&just(int(i64::MIN..=i64::MAX)));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, none(), IsReachable(true));
    assert_reach!(tree, wildcard("_"), IsReachable(false));
    assert!(tree.is_exhaustive() && dbg!(tree.generate_missing_patterns()).is_empty());
}

#[test]
fn tuple_of_strings() {
    let mut tree = PatternTree::from_pattern(&tuple([string("a"), string("a")]));
    assert_reach!(tree, tuple([string("a"), string("b")]), IsReachable(true));
    assert_reach!(tree, tuple([string("a"), string("a")]), IsReachable(false));
    assert_reach!(tree, tuple([string("a"), wildcard("_")]), IsReachable(true));
    assert_reach!(tree, tuple([string("a"), string("c")]), IsReachable(false));
    assert!(!tree.generate_missing_patterns().is_empty());
    assert_reach!(tree, wildcard("_"), IsReachable(true));
    assert_reach!(tree, tuple([string("b"), string("b")]), IsReachable(false));
    assert!(tree.is_exhaustive() && dbg!(tree.generate_missing_patterns()).is_empty());
}

#[test]
fn lots_of_ranges() {
    let mut tree = PatternTree::from_pattern(&tuple([int(0..=0), int(1..=1)]));
    assert_reach!(tree, tuple([int(1..=1), int(1..=1)]), IsReachable(true));
    assert_reach!(tree, tuple([int(0..=1), int(1..=1)]), IsReachable(false));
    assert_reach!(tree, tuple([wildcard("_"), int(2..=2)]), IsReachable(true));
    assert_reach!(tree, tuple([int(2..=2), int(3..=3)]), IsReachable(true));
    assert_reach!(tree, tuple([int(2..=2), int(2..=3)]), IsReachable(false));
    assert!(!tree.generate_missing_patterns().is_empty());
}

#[test]
fn init_from_wc() {
    let mut tree = PatternTree::from_pattern(&wildcard("_"));
    println!(" !! init tree:\n{}", &tree);
    assert_reach!(tree, tuple([int(0..=0), int(1..=1)]), IsReachable(false));
    let missing = tree.generate_missing_patterns();
    assert!(missing.is_empty());
}

#[test]
fn min_max() {
    use super::tree::merge::{signed_max, signed_min, unsigned_max};
    assert_eq!(
        signed_min(64)..=signed_max(64),
        i64::MIN as i128..=i64::MAX as i128
    );
    assert_eq!(unsigned_max(32), u32::MAX as u128);
}

#[test]
fn unknown_wildcard() {
    let tree = PatternTree::from_pattern(&wildcard("_"));
    let missing = tree.generate_missing_patterns();
    if !missing.is_empty() {
        panic!("{:?}", missing);
    }
}

#[test]
fn tuple_of_wildcards() {
    let mut tree = PatternTree::from_pattern(&tuple([int(0..=0), int(1..=1)]));
    assert_reach!(
        tree,
        tuple([wildcard("a"), wildcard("b")]),
        IsReachable(true)
    );
    assert_reach!(tree, tuple([int(0..=0), int(1..=1)]), IsReachable(false));
    assert!(tree.is_exhaustive());
    let missing = tree.generate_missing_patterns();
    if !missing.is_empty() {
        panic!("{:?}", missing);
    }
}

#[test]
fn empty_wildcard() {
    let tree = PatternTree::from_pattern(&tuple([]));
    let missing = tree.generate_missing_patterns();
    if !missing.is_empty() {
        panic!("{:?}", missing);
    }
}
