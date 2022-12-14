use itertools::Itertools;
use std::collections::VecDeque;
use std::fmt;
use std::fmt::Debug;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ops::RangeInclusive;

/// The constructors that can be used as patterns defined by you
///
///  * Lenghted
///      are compared by their parameter count
///      (such as slices)
///
///  * Constant
///      always matches with itself and must always have the same
///      amount of parameters
///
///  * Variant
///      sum type variants
///
///  * Infinite
///      have an infinite amount of variants, but can still be equal to each other
///      (such as strings)
///      
/// Even though PartialEq is only strictly required for `Infinite`, we use plenty of debug
/// assertion to verify that your type checker didn't leave any holes which depends on PartialEq for
/// the other associated types as well.
pub trait Constructors: Clone + std::fmt::Debug {
    type Lengthed: Clone + Debug + PartialEq;
    type Constant: Clone + Debug + PartialEq + ConstantConstructor;
    type SumType: Clone + Debug + PartialEq + SumtypeConstructor;
    type Infinite: Clone + Debug + PartialEq;
    type Wildcard: Clone + Debug + Default;
}

pub trait SumtypeConstructor {
    fn max(&self) -> u64;
    fn params_for(&self, tag: u64) -> usize;
}

pub trait ConstantConstructor {
    fn len_requirement(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct Pattern<C: Constructors> {
    pub constr: Constructor<C>,
    pub params: Vec<Self>,
}

impl<C: Constructors> Pattern<C> {
    pub fn new(constr: Constructor<C>) -> Self {
        Pattern { constr, params: vec![] }
    }

    #[must_use]
    pub fn with_params(mut self, params: Vec<Self>) -> Self {
        #[cfg(debug_assertions)]
        {
            assert!(self.params.is_empty());
            match &self.constr {
                Constructor::Constant(constr) => {
                    assert_eq!(
                        params.len(),
                        constr.len_requirement(),
                        "constant takes the wrong amount of parameters"
                    );
                }
                Constructor::Variant { type_, tag } => assert_eq!(
                    params.len(),
                    type_.params_for(*tag),
                    "sum type takes the wrong amount of parameters"
                ),
                _ => {}
            }
        }

        self.params = params;
        self
    }

    pub fn wildcard(wc: C::Wildcard) -> Self {
        Pattern {
            constr: Constructor::Wildcard(wc),
            params: vec![],
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Constructor<C: Constructors> {
    SignedInteger {
        range: RangeInclusive<i128>,
        bitsize: u8,
    },
    UnsignedInteger {
        range: RangeInclusive<u128>,
        bitsize: u8,
    },
    Variant {
        type_: C::SumType,
        tag: u64,
    },
    Infinite(C::Infinite),
    Lenghted(C::Lengthed),
    Constant(C::Constant),
    Wildcard(C::Wildcard),
}

#[derive(Clone, Debug)]
pub struct FlatPatterns<C: Constructors> {
    buf: VecDeque<(Constructor<C>, usize)>,
}

impl<C: Constructors> Deref for FlatPatterns<C> {
    type Target = VecDeque<(Constructor<C>, usize)>;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}
impl<C: Constructors> DerefMut for FlatPatterns<C> {
    fn deref_mut(&mut self) -> &mut VecDeque<(Constructor<C>, usize)> {
        &mut self.buf
    }
}

impl<C: Constructors> Pattern<C> {
    pub fn flatten(&self) -> FlatPatterns<C> {
        let mut flat = FlatPatterns {
            buf: VecDeque::with_capacity(self.params.len() + 1),
        };
        flat.include(self);
        flat
    }
}

impl<C: Constructors> FlatPatterns<C> {
    fn include(&mut self, p: &Pattern<C>) {
        self.push_back((p.constr.clone(), p.params.len()));
        p.params.iter().for_each(|p| self.include(p))
    }
}

impl<C: Constructors> Iterator for FlatPatterns<C> {
    type Item = (Constructor<C>, usize);

    fn next(&mut self) -> Option<Self::Item> {
        self.buf.pop_front()
    }
}

impl<S: fmt::Display, C: Constructors<SumType = S>> fmt::Display for Pattern<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.params[..] {
            [] => fmt::Display::fmt(&self.constr, f),
            params => write!(f, "{} {}", self.constr, params.iter().format(" ")),
        }
    }
}

impl<S: fmt::Display, C: Constructors<SumType = S>> fmt::Display for Constructor<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Constructor::Variant { type_, tag } => {
                write!(f, "{}[{}]", type_, tag)
            }
            Constructor::Wildcard(wc) => write!(f, "{:?}", wc),
            _ => todo!(),
        }
    }
}
