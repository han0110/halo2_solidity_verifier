use crate::codegen::{
    template::Halo2VerifyingKey,
    BatchOpenScheme::{self, Bdfg21, Gwc19},
};
use halo2_proofs::{
    halo2curves::ff::PrimeField,
    plonk::{Any, Column, ConstraintSystem},
};
use itertools::{chain, izip, Itertools};
use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
    ops::{Add, Sub},
};

#[derive(Debug)]
pub(crate) struct ConstraintSystemMeta {
    pub(crate) num_fixeds: usize,
    pub(crate) permutation_columns: Vec<Column<Any>>,
    pub(crate) permutation_chunk_len: usize,
    pub(crate) num_lookup_permuteds: usize,
    pub(crate) num_permutation_zs: usize,
    pub(crate) num_lookup_zs: usize,
    pub(crate) num_quotients: usize,
    pub(crate) advice_queries: Vec<(usize, i32)>,
    pub(crate) fixed_queries: Vec<(usize, i32)>,
    pub(crate) num_evals: usize,
    pub(crate) num_user_advices: Vec<usize>,
    pub(crate) num_user_challenges: Vec<usize>,
    pub(crate) advice_index: Vec<usize>,
    pub(crate) challenge_index: Vec<usize>,
    pub(crate) rotation_last: i32,
}

impl ConstraintSystemMeta {
    pub(crate) fn new<F: PrimeField>(cs: &ConstraintSystem<F>) -> Self {
        let num_fixeds = cs.num_fixed_columns();
        let permutation_columns = cs.permutation().get_columns();
        let permutation_chunk_len = cs.degree() - 2;
        let num_lookup_permuteds = 2 * cs.lookups().len();
        let num_permutation_zs = cs
            .permutation()
            .get_columns()
            .chunks(cs.degree() - 2)
            .count();
        let num_lookup_zs = cs.lookups().len();
        let num_quotients = cs.degree() - 1;
        let advice_queries = cs
            .advice_queries()
            .iter()
            .map(|(column, rotation)| (column.index(), rotation.0))
            .collect_vec();
        let fixed_queries = cs
            .fixed_queries()
            .iter()
            .map(|(column, rotation)| (column.index(), rotation.0))
            .collect_vec();
        let num_evals = advice_queries.len()
            + fixed_queries.len()
            + 1
            + cs.permutation().get_columns().len()
            + (3 * num_permutation_zs - 1)
            + 5 * cs.lookups().len();
        let num_phase = *cs.advice_column_phase().iter().max().unwrap_or(&0) as usize + 1;
        let remapping = |phase: Vec<u8>| {
            let nums = phase.iter().fold(vec![0; num_phase], |mut nums, phase| {
                nums[*phase as usize] += 1;
                nums
            });
            let offsets = nums
                .iter()
                .take(num_phase - 1)
                .fold(vec![0], |mut offsets, n| {
                    offsets.push(offsets.last().unwrap() + n);
                    offsets
                });
            let index = phase
                .iter()
                .scan(offsets, |state, phase| {
                    let index = state[*phase as usize];
                    state[*phase as usize] += 1;
                    Some(index)
                })
                .collect::<Vec<_>>();
            (nums, index)
        };
        let (num_user_advices, advice_index) = remapping(cs.advice_column_phase());
        let (num_user_challenges, challenge_index) = remapping(cs.challenge_phase());
        let rotation_last = -(cs.blinding_factors() as i32 + 1);
        Self {
            num_fixeds,
            permutation_columns,
            permutation_chunk_len,
            num_lookup_permuteds,
            num_permutation_zs,
            num_lookup_zs,
            num_quotients,
            advice_queries,
            fixed_queries,
            num_evals,
            num_user_advices,
            num_user_challenges,
            advice_index,
            challenge_index,
            rotation_last,
        }
    }

    pub(crate) fn num_advices(&self) -> Vec<usize> {
        chain![
            self.num_user_advices.iter().cloned(),
            [
                self.num_lookup_permuteds,                        // lookup permuted
                self.num_permutation_zs + self.num_lookup_zs + 1, // permutation and lookup grand products, random
                self.num_quotients,                               // quotients
            ],
        ]
        .collect()
    }

    pub(crate) fn num_challenges(&self) -> Vec<usize> {
        let mut num_challenges = self.num_user_challenges.clone();
        *num_challenges.last_mut().unwrap() += 1; // theta
        num_challenges.extend([
            2, // beta, gamma
            1, // y
            1, // x
        ]);
        num_challenges
    }

    pub fn num_permutations(&self) -> usize {
        self.permutation_columns.len()
    }

    pub fn num_lookups(&self) -> usize {
        self.num_lookup_zs
    }

    pub(crate) fn proof_len(&self, scheme: BatchOpenScheme) -> usize {
        self.num_advices().iter().sum::<usize>() * 0x40
            + self.num_evals * 0x20
            + self.batch_open_proof_len(scheme)
    }

    pub(crate) fn batch_open_proof_len(&self, scheme: BatchOpenScheme) -> usize {
        match scheme {
            Bdfg21 => 2 * 0x40,
            Gwc19 => {
                unimplemented!()
            }
        }
    }

    pub(crate) fn num_batch_open_challenges(&self, scheme: BatchOpenScheme) -> usize {
        match scheme {
            Bdfg21 => 3,
            Gwc19 => {
                unimplemented!()
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct Data {
    pub(crate) challenge_mptr: usize,
    pub(crate) theta_mptr: usize,
    pub(crate) instance_eval_mptr: usize,
    pub(crate) quotient_comm_cptr: usize,
    pub(crate) w_cptr: usize,

    pub(crate) fixed_comms: Vec<EcPoint>,
    pub(crate) permutation_comms: HashMap<Column<Any>, EcPoint>,
    pub(crate) advice_comms: Vec<EcPoint>,
    pub(crate) lookup_permuted_comms: Vec<(EcPoint, EcPoint)>,
    pub(crate) permutation_z_comms: Vec<EcPoint>,
    pub(crate) lookup_z_comms: Vec<EcPoint>,
    pub(crate) random_comm: EcPoint,
    pub(crate) quotient_comm: EcPoint,

    pub(crate) challenges: Vec<U256Expr>,

    pub(crate) instance_eval: U256Expr,
    pub(crate) advice_evals: HashMap<(usize, i32), U256Expr>,
    pub(crate) fixed_evals: HashMap<(usize, i32), U256Expr>,
    pub(crate) random_eval: U256Expr,
    pub(crate) permutation_evals: HashMap<Column<Any>, U256Expr>,
    pub(crate) permutation_z_evals: Vec<(U256Expr, U256Expr, U256Expr)>,
    pub(crate) lookup_evals: Vec<(U256Expr, U256Expr, U256Expr, U256Expr, U256Expr)>,
    pub(crate) quotient_eval: U256Expr,
}

impl Data {
    pub(crate) fn new(
        meta: &ConstraintSystemMeta,
        scheme: BatchOpenScheme,
        vk: &Halo2VerifyingKey,
        vk_mptr: usize,
        proof_cptr: usize,
    ) -> Self {
        let fixed_comm_mptr = vk_mptr + vk.constants.len() * 0x20;
        let permutation_comm_mptr = fixed_comm_mptr + vk.fixed_comms.len() * 0x40;
        let challenge_mptr = permutation_comm_mptr + vk.permutation_comms.len() * 0x40;
        let theta_mptr = challenge_mptr + 0x20 * meta.num_user_challenges.iter().sum::<usize>();
        let instance_eval_mptr = theta_mptr + (5 + meta.num_batch_open_challenges(scheme)) * 0x20;

        let advice_comm_cptr = proof_cptr;
        let lookup_permuted_comm_cptr = advice_comm_cptr + meta.advice_index.len() * 0x40;
        let permutation_z_comm_cptr = lookup_permuted_comm_cptr + meta.num_lookup_permuteds * 0x40;
        let lookup_z_comm_cptr = permutation_z_comm_cptr + meta.num_permutation_zs * 0x40;
        let random_comm_cptr = lookup_z_comm_cptr + meta.num_lookup_zs * 0x40;
        let quotient_comm_cptr = random_comm_cptr + 0x40;

        let eval_cptr = quotient_comm_cptr + meta.num_quotients * 0x40;
        let advice_eval_cptr = eval_cptr;
        let fixed_eval_cptr = advice_eval_cptr + meta.advice_queries.len() * 0x20;
        let random_eval_cptr = fixed_eval_cptr + meta.fixed_queries.len() * 0x20;
        let permutation_eval_cptr = random_eval_cptr + 0x20;
        let permutation_z_eval_cptr = permutation_eval_cptr + meta.num_permutations() * 0x20;
        let lookup_eval_cptr = permutation_z_eval_cptr + (3 * meta.num_permutation_zs - 1) * 0x20;
        let w_cptr = lookup_eval_cptr + 5 * meta.num_lookups() * 0x20;

        let fixed_comms = (fixed_comm_mptr..)
            .step_by(0x40)
            .take(meta.num_fixeds)
            .map(EcPoint::mptr)
            .collect();
        let permutation_comms = izip!(
            meta.permutation_columns.iter().cloned(),
            (permutation_comm_mptr..).step_by(0x40).map(EcPoint::mptr)
        )
        .collect();
        let advice_comms = meta
            .advice_index
            .iter()
            .map(|idx| EcPoint::cptr(advice_comm_cptr + idx * 0x40))
            .collect();
        let lookup_permuted_comms = (lookup_permuted_comm_cptr..)
            .step_by(0x40)
            .take(meta.num_lookup_permuteds)
            .map(EcPoint::cptr)
            .tuples()
            .collect();
        let permutation_z_comms = (permutation_z_comm_cptr..)
            .step_by(0x40)
            .take(meta.num_permutation_zs)
            .map(EcPoint::cptr)
            .collect();
        let lookup_z_comms = (lookup_z_comm_cptr..)
            .step_by(0x40)
            .take(meta.num_lookup_zs)
            .map(EcPoint::cptr)
            .collect();
        let random_comm = EcPoint::cptr(random_comm_cptr);
        let quotient_comm = EcPoint::mptr_xy("H_X_MPTR", "H_Y_MPTR");

        let challenges = meta
            .challenge_index
            .iter()
            .map(|idx| U256Expr::mptr(challenge_mptr + idx * 0x20))
            .collect_vec();
        let instance_eval = U256Expr::mptr("INSTANCE_EVAL_MPTR");
        let advice_evals = izip!(
            meta.advice_queries.iter().cloned(),
            (advice_eval_cptr..).step_by(0x20).map(U256Expr::cptr)
        )
        .collect();
        let fixed_evals = izip!(
            meta.fixed_queries.iter().cloned(),
            (fixed_eval_cptr..).step_by(0x20).map(U256Expr::cptr)
        )
        .collect();
        let random_eval = U256Expr::cptr(random_eval_cptr);
        let permutation_evals = meta
            .permutation_columns
            .iter()
            .cloned()
            .zip((permutation_eval_cptr..).step_by(0x20).map(U256Expr::cptr))
            .collect();
        let permutation_z_evals = (permutation_z_eval_cptr..)
            .step_by(0x20)
            .map(U256Expr::cptr)
            .take(3 * meta.num_permutation_zs)
            .tuples()
            .collect_vec();
        let lookup_evals = (lookup_eval_cptr..)
            .step_by(0x20)
            .map(U256Expr::cptr)
            .take(5 * meta.num_lookup_zs)
            .tuples()
            .collect_vec();
        let quotient_eval = U256Expr::mptr("H_EVAL_MPTR");

        Self {
            challenge_mptr,
            theta_mptr,
            instance_eval_mptr,
            quotient_comm_cptr,
            w_cptr,

            fixed_comms,
            permutation_comms,
            advice_comms,
            lookup_permuted_comms,
            permutation_z_comms,
            lookup_z_comms,
            random_comm,
            quotient_comm,

            challenges,

            instance_eval,
            advice_evals,
            fixed_evals,
            permutation_evals,
            permutation_z_evals,
            lookup_evals,
            random_eval,
            quotient_eval,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ptr {
    Literal(usize),
    Identifier(&'static str),
}

impl Ptr {
    pub(crate) fn range_from(&self) -> impl Iterator<Item = Ptr> + '_ {
        (0..).map(|idx| *self + idx)
    }

    fn as_usize(&self) -> usize {
        match self {
            Ptr::Literal(literal) => *literal,
            _ => unreachable!(),
        }
    }
}

impl Default for Ptr {
    fn default() -> Self {
        Self::Literal(0)
    }
}

impl Display for Ptr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Ptr::Literal(literal) => {
                write!(f, "0x{literal:x}")
            }
            Ptr::Identifier(ident) => {
                write!(f, "{ident}")
            }
        }
    }
}

impl From<&'static str> for Ptr {
    fn from(ident: &'static str) -> Self {
        Ptr::Identifier(ident)
    }
}

impl From<usize> for Ptr {
    fn from(literal: usize) -> Self {
        Ptr::Literal(literal)
    }
}

impl Add<usize> for Ptr {
    type Output = Ptr;

    fn add(self, rhs: usize) -> Self::Output {
        (self.as_usize() + rhs * 0x20).into()
    }
}

impl Sub<usize> for Ptr {
    type Output = Ptr;

    fn sub(self, rhs: usize) -> Self::Output {
        (self.as_usize() - rhs * 0x20).into()
    }
}

macro_rules! ptr {
    () => {
        Ptr::default()
    };
    ($value:expr) => {
        Ptr::default() + $value
    };
}

pub(crate) use ptr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum U256Expr {
    Memory(Ptr),
    Calldata(Ptr),
}

impl U256Expr {
    pub fn mptr(ptr: impl Into<Ptr>) -> Self {
        U256Expr::Memory(ptr.into())
    }

    pub fn cptr(ptr: impl Into<Ptr>) -> Self {
        U256Expr::Calldata(ptr.into())
    }

    fn ptr(&self) -> Ptr {
        match self {
            U256Expr::Memory(mptr) => *mptr,
            U256Expr::Calldata(cptr) => *cptr,
        }
    }
}

impl Display for U256Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            U256Expr::Memory(mptr) => {
                write!(f, "mload({mptr})")
            }
            U256Expr::Calldata(cptr) => {
                write!(f, "calldataload({cptr})")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EcPoint {
    x: U256Expr,
    y: U256Expr,
}

impl EcPoint {
    pub(crate) fn mptr_xy(x: impl Into<Ptr>, y: impl Into<Ptr>) -> Self {
        Self {
            x: U256Expr::mptr(x),
            y: U256Expr::mptr(y),
        }
    }

    pub(crate) fn mptr(mptr: usize) -> Self {
        Self::mptr_xy(mptr, mptr + 0x20)
    }

    pub(crate) fn cptr(cptr: usize) -> Self {
        Self {
            x: U256Expr::cptr(cptr),
            y: U256Expr::cptr(cptr + 0x20),
        }
    }

    pub(crate) fn x(&self) -> &U256Expr {
        &self.x
    }

    pub(crate) fn y(&self) -> &U256Expr {
        &self.y
    }
}

impl Add<usize> for EcPoint {
    type Output = EcPoint;

    fn add(self, rhs: usize) -> Self::Output {
        let x = self.x().ptr() + 2 * rhs;
        let y = self.y().ptr() + 2 * rhs;
        assert_eq!(x + 1, y);
        match self.x {
            U256Expr::Memory(_) => Self {
                x: U256Expr::Memory(x),
                y: U256Expr::Memory(y),
            },
            U256Expr::Calldata(_) => Self {
                x: U256Expr::Calldata(x),
                y: U256Expr::Calldata(y),
            },
        }
    }
}

pub(crate) fn indent<const N: usize>(lines: impl IntoIterator<Item = String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| format!("{}{line}", " ".repeat(N * 4)))
        .collect()
}

pub(crate) fn code_block<const N: usize>(lines: impl IntoIterator<Item = String>) -> Vec<String> {
    chain![
        [format!("{}{{", " ".repeat((N - 1) * 4))],
        indent::<N>(lines),
        [format!("{}}}", " ".repeat((N - 1) * 4))],
    ]
    .collect()
}
