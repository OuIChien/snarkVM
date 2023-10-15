// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::sonic_pc::{
    BatchLCProof,
    BatchProof,
    Commitment,
    Evaluations,
    LabeledCommitment,
    QuerySet,
    Randomness,
    SonicKZG10,
};
use crate::{
    fft::DensePolynomial,
    polycommit::{
        kzg10::DegreeInfo,
        sonic_pc::{LabeledPolynomial, LabeledPolynomialWithBasis, LinearCombination},
        PCError,
    },
    srs::UniversalVerifier,
    AlgebraicSponge,
};
use snarkvm_curves::PairingEngine;
use snarkvm_fields::{One, Zero};
use snarkvm_utilities::rand::{TestRng, Uniform};
use std::collections::HashSet;

use itertools::Itertools;
use rand::{
    distributions::{self, Distribution},
    Rng,
};
use std::marker::PhantomData;

#[derive(Default)]
struct TestInfo {
    num_iters: usize,
    max_degree: Option<usize>,
    supported_degree: Option<usize>,
    num_polynomials: usize,
    enforce_degree_bounds: bool,
    max_num_queries: usize,
    num_equations: Option<usize>,
}

pub struct TestComponents<E: PairingEngine, S: AlgebraicSponge<E::Fq, 2>> {
    pub universal_verifier: UniversalVerifier<E>,
    pub commitments: Vec<LabeledCommitment<Commitment<E>>>,
    pub query_set: QuerySet<E::Fr>,
    pub evaluations: Evaluations<E::Fr>,
    pub batch_lc_proof: Option<BatchLCProof<E>>,
    pub batch_proof: Option<BatchProof<E>>,
    pub randomness: Vec<Randomness<E>>,
    _sponge: PhantomData<S>,
}

pub fn bad_degree_bound_test<E: PairingEngine, S: AlgebraicSponge<E::Fq, 2>>() -> Result<(), PCError> {
    let rng = &mut TestRng::default();
    let max_degree = 100;
    let hiding_bound = 1;
    let pp = SonicKZG10::<E, S>::load_srs(max_degree)?;
    let universal_verifier = pp.to_universal_verifier().unwrap();

    for _ in 0..10 {
        let supported_degree = distributions::Uniform::from(1..=max_degree).sample(rng);
        assert!(max_degree >= supported_degree, "max_degree < supported_degree");

        let mut labels = Vec::new();
        let mut polynomials = Vec::new();
        let mut degree_bounds = HashSet::new();

        for i in 0..10 {
            let label = format!("Test{i}");
            labels.push(label.clone());
            let poly = DensePolynomial::rand(supported_degree, rng);

            let degree_bound = 1usize;
            degree_bounds.insert(degree_bound);

            polynomials.push(LabeledPolynomial::new(label, poly, Some(degree_bound), Some(hiding_bound)))
        }

        let degree_info = DegreeInfo {
            max_degree,
            max_fft_size: supported_degree,
            degree_bounds: Some(degree_bounds),
            hiding_bound,
            lagrange_sizes: None,
        };
        let universal_prover = &pp.to_universal_prover(degree_info).unwrap();

        let (comms, rands) =
            SonicKZG10::<E, S>::commit(universal_prover, polynomials.iter().map(Into::into), Some(rng))?;

        let mut query_set = QuerySet::new();
        let mut values = Evaluations::new();
        let point = E::Fr::rand(rng);
        for (i, label) in labels.iter().enumerate() {
            query_set.insert((label.clone(), ("rand".into(), point)));
            let value = polynomials[i].evaluate(point);
            values.insert((label.clone(), point), value);
        }

        let mut sponge_for_open = S::new();
        let proof = SonicKZG10::batch_open(universal_prover, &polynomials, &query_set, &rands, &mut sponge_for_open)?;
        let mut sponge_for_check = S::new();
        let result =
            SonicKZG10::batch_check(&universal_verifier, &comms, &query_set, &values, &proof, &mut sponge_for_check)?;
        assert!(result, "proof was incorrect, Query set: {query_set:#?}");
    }
    Ok(())
}

pub fn lagrange_test_template<E: PairingEngine, S: AlgebraicSponge<E::Fq, 2>>()
-> Result<Vec<TestComponents<E, S>>, PCError> {
    let num_iters = 10usize;
    let max_degree = 256usize;
    let supported_degree = 127usize;
    let eval_size = 128usize;
    let num_polynomials = 1usize;
    let max_num_queries = 2usize;
    let mut test_components = Vec::new();

    let rng = &mut TestRng::default();
    let pp = SonicKZG10::<E, S>::load_srs(max_degree)?;

    for _ in 0..num_iters {
        let universal_verifier = pp.to_universal_verifier().unwrap();
        assert!(max_degree >= supported_degree, "max_degree < supported_degree");
        let mut polynomials = Vec::new();
        let mut lagrange_polynomials = Vec::new();
        let mut supported_lagrange_sizes = HashSet::new();
        let degree_bounds = HashSet::new();
        let hiding_bound = 1;

        let mut labels = Vec::new();

        // Generate polynomials
        let num_points_in_query_set = distributions::Uniform::from(1..=max_num_queries).sample(rng);
        for i in 0..num_polynomials {
            let label = format!("Test{i}");
            labels.push(label.clone());
            let eval_size: usize = distributions::Uniform::from(1..eval_size).sample(rng).next_power_of_two();
            let mut evals = vec![E::Fr::zero(); eval_size];
            for e in &mut evals {
                *e = E::Fr::rand(rng);
            }
            let domain = crate::fft::EvaluationDomain::new(evals.len()).unwrap();
            let evals = crate::fft::Evaluations::from_vec_and_domain(evals, domain);
            let poly = evals.interpolate_by_ref();
            supported_lagrange_sizes.insert(domain.size());
            assert_eq!(poly.evaluate_over_domain_by_ref(domain), evals);

            let degree_bound = None;

            polynomials.push(LabeledPolynomial::new(label.clone(), poly, degree_bound, Some(hiding_bound)));
            lagrange_polynomials.push(LabeledPolynomialWithBasis::new_lagrange_basis(label, evals, Some(hiding_bound)))
        }
        let supported_hiding_bound = polynomials.iter().map(|p| p.hiding_bound().unwrap_or(0)).max().unwrap_or(0);
        assert_eq!(supported_hiding_bound, 1);
        let degree_info = DegreeInfo {
            max_degree,
            max_fft_size: supported_degree,
            degree_bounds: Some(degree_bounds),
            hiding_bound,
            lagrange_sizes: Some(supported_lagrange_sizes),
        };
        let universal_prover = &pp.to_universal_prover(degree_info).unwrap();

        let (comms, rands) = SonicKZG10::<E, S>::commit(universal_prover, lagrange_polynomials, Some(rng)).unwrap();

        // Construct query set
        let mut query_set = QuerySet::new();
        let mut values = Evaluations::new();
        // let mut point = E::Fr::one();
        for point_id in 0..num_points_in_query_set {
            let point = E::Fr::rand(rng);
            for (polynomial, label) in polynomials.iter().zip_eq(labels.iter()) {
                query_set.insert((label.clone(), (format!("rand_{point_id}"), point)));
                let value = polynomial.evaluate(point);
                values.insert((label.clone(), point), value);
            }
        }
        println!("Generated query set");

        let mut sponge_for_open = S::new();
        let proof = SonicKZG10::batch_open(universal_prover, &polynomials, &query_set, &rands, &mut sponge_for_open)?;
        let mut sponge_for_check = S::new();
        let result =
            SonicKZG10::batch_check(&universal_verifier, &comms, &query_set, &values, &proof, &mut sponge_for_check)?;
        if !result {
            println!("Failed with {num_polynomials} polynomials, num_points_in_query_set: {num_points_in_query_set:?}");
            println!("Degree of polynomials:");
            for poly in polynomials {
                println!("Degree: {:?}", poly.degree());
            }
        }
        assert!(result, "proof was incorrect, Query set: {query_set:#?}");

        test_components.push(TestComponents {
            universal_verifier,
            commitments: comms,
            query_set,
            evaluations: values,
            batch_lc_proof: None,
            batch_proof: Some(proof),
            randomness: rands,
            _sponge: PhantomData,
        });
    }
    Ok(test_components)
}

fn test_template<E, S>(info: TestInfo) -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let TestInfo {
        num_iters,
        max_degree,
        supported_degree,
        num_polynomials,
        enforce_degree_bounds,
        max_num_queries,
        ..
    } = info;

    let mut test_components = Vec::new();

    let rng = &mut TestRng::default();
    let max_degree = max_degree.unwrap_or_else(|| distributions::Uniform::from(8..=64).sample(rng));
    let pp = SonicKZG10::<E, S>::load_srs(max_degree)?;
    let supported_degree_bounds = vec![1 << 10, 1 << 15, 1 << 20, 1 << 25, 1 << 30];
    let hiding_bound = 1;

    for _ in 0..num_iters {
        let universal_verifier = pp.to_universal_verifier().unwrap();
        let supported_degree =
            supported_degree.unwrap_or_else(|| distributions::Uniform::from(4..=max_degree).sample(rng));
        assert!(max_degree >= supported_degree, "max_degree < supported_degree");
        let mut polynomials = Vec::new();
        let mut degree_bounds = HashSet::new();

        let mut labels = Vec::new();
        println!("Sampled supported degree");

        // Generate polynomials
        let num_points_in_query_set = distributions::Uniform::from(1..=max_num_queries).sample(rng);
        for i in 0..num_polynomials {
            let label = format!("Test{i}");
            labels.push(label.clone());
            let degree = distributions::Uniform::from(1..=supported_degree).sample(rng);
            let poly = DensePolynomial::rand(degree, rng);

            let supported_degree_bounds_after_trimmed = supported_degree_bounds
                .iter()
                .copied()
                .filter(|x| *x >= degree && *x < supported_degree)
                .collect::<Vec<usize>>();

            let compute_degree_bound =
                enforce_degree_bounds && !supported_degree_bounds_after_trimmed.is_empty() && rng.gen();
            let degree_bound = compute_degree_bound.then(|| {
                let range = distributions::Uniform::from(0..supported_degree_bounds_after_trimmed.len());
                let idx = range.sample(rng);
                supported_degree_bounds_after_trimmed[idx]
            });
            if let Some(degree_bound) = degree_bound {
                degree_bounds.insert(degree_bound);
            }
            polynomials.push(LabeledPolynomial::new(label, poly, degree_bound, Some(hiding_bound)))
        }
        let supported_hiding_bound = polynomials.iter().map(|p| p.hiding_bound().unwrap_or(0)).max().unwrap_or(0);
        assert_eq!(supported_hiding_bound, 1);
        let degree_info = DegreeInfo {
            max_degree,
            max_fft_size: supported_degree,
            degree_bounds: Some(degree_bounds),
            hiding_bound,
            lagrange_sizes: None,
        };
        let universal_prover = &pp.to_universal_prover(degree_info).unwrap();

        let (comms, rands) =
            SonicKZG10::<E, S>::commit(universal_prover, polynomials.iter().map(Into::into), Some(rng))?;

        // Construct query set
        let mut query_set = QuerySet::new();
        let mut values = Evaluations::new();
        // let mut point = E::Fr::one();
        for point_id in 0..num_points_in_query_set {
            let point = E::Fr::rand(rng);
            for (polynomial, label) in polynomials.iter().zip_eq(labels.iter()) {
                query_set.insert((label.clone(), (format!("rand_{point_id}"), point)));
                let value = polynomial.evaluate(point);
                values.insert((label.clone(), point), value);
            }
        }
        println!("Generated query set");

        let mut sponge_for_open = S::new();
        let proof = SonicKZG10::batch_open(universal_prover, &polynomials, &query_set, &rands, &mut sponge_for_open)?;
        let mut sponge_for_check = S::new();
        let result =
            SonicKZG10::batch_check(&universal_verifier, &comms, &query_set, &values, &proof, &mut sponge_for_check)?;
        if !result {
            println!("Failed with {num_polynomials} polynomials, num_points_in_query_set: {num_points_in_query_set:?}");
            println!("Degree of polynomials:");
            for poly in polynomials {
                println!("Degree: {:?}", poly.degree());
            }
        }
        assert!(result, "proof was incorrect, Query set: {query_set:#?}");

        test_components.push(TestComponents {
            universal_verifier,
            commitments: comms,
            query_set,
            evaluations: values,
            batch_lc_proof: None,
            batch_proof: Some(proof),
            randomness: rands,
            _sponge: PhantomData,
        });
    }
    Ok(test_components)
}

fn equation_test_template<E: PairingEngine, S: AlgebraicSponge<E::Fq, 2>>(
    info: TestInfo,
) -> Result<Vec<TestComponents<E, S>>, PCError> {
    let TestInfo {
        num_iters,
        max_degree,
        supported_degree,
        num_polynomials,
        enforce_degree_bounds,
        max_num_queries,
        num_equations,
    } = info;

    let mut test_components = Vec::new();

    let rng = &mut TestRng::default();
    let max_degree = max_degree.unwrap_or_else(|| distributions::Uniform::from(8..=64).sample(rng));
    let pp = SonicKZG10::<E, S>::load_srs(max_degree)?;
    let supported_degree_bounds = vec![1 << 10, 1 << 15, 1 << 20, 1 << 25, 1 << 30];
    let hiding_bound = 1;

    for _ in 0..num_iters {
        let universal_verifier = pp.to_universal_verifier().unwrap();
        let supported_degree =
            supported_degree.unwrap_or_else(|| distributions::Uniform::from(4..=max_degree).sample(rng));
        assert!(max_degree >= supported_degree, "max_degree < supported_degree");
        let mut polynomials = Vec::new();
        let mut degree_bounds = HashSet::new();

        let mut labels = Vec::new();
        println!("Sampled supported degree");

        // Generate polynomials
        let num_points_in_query_set = distributions::Uniform::from(1..=max_num_queries).sample(rng);
        for i in 0..num_polynomials {
            let label = format!("Test{i}");
            labels.push(label.clone());
            let degree = distributions::Uniform::from(1..=supported_degree).sample(rng);
            let poly = DensePolynomial::rand(degree, rng);

            let supported_degree_bounds_after_trimmed = supported_degree_bounds
                .iter()
                .copied()
                .filter(|x| *x >= degree && *x < supported_degree)
                .collect::<Vec<usize>>();

            let compute_degree_bound =
                enforce_degree_bounds && !supported_degree_bounds_after_trimmed.is_empty() && rng.gen();
            let degree_bound = compute_degree_bound.then(|| {
                let range = distributions::Uniform::from(0..supported_degree_bounds_after_trimmed.len());
                let idx = range.sample(rng);
                supported_degree_bounds_after_trimmed[idx]
            });
            if let Some(degree_bound) = degree_bound {
                degree_bounds.insert(degree_bound);
            }

            polynomials.push(LabeledPolynomial::new(label, poly, degree_bound, Some(hiding_bound)))
        }
        let supported_hiding_bound = polynomials.iter().map(|p| p.hiding_bound().unwrap_or(0)).max().unwrap_or(0);
        assert_eq!(supported_hiding_bound, 1);
        let degree_info = DegreeInfo {
            max_degree,
            max_fft_size: supported_degree,
            degree_bounds: Some(degree_bounds),
            hiding_bound,
            lagrange_sizes: None,
        };
        let universal_prover = &pp.to_universal_prover(degree_info).unwrap();

        let (comms, rands) =
            SonicKZG10::<E, S>::commit(universal_prover, polynomials.iter().map(Into::into), Some(rng))?;

        // Let's construct our equations
        let mut linear_combinations = Vec::new();
        let mut query_set = QuerySet::new();
        let mut values = Evaluations::new();
        for i in 0..num_points_in_query_set {
            let point = E::Fr::rand(rng);
            for j in 0..num_equations.unwrap() {
                let label = format!("query {i} eqn {j}");
                let mut lc = LinearCombination::empty(label.clone());

                let mut value = E::Fr::zero();
                let should_have_degree_bounds: bool = rng.gen();
                for (k, label) in labels.iter().enumerate() {
                    if should_have_degree_bounds {
                        value += &polynomials[k].evaluate(point);
                        lc.add(E::Fr::one(), label.clone());
                        break;
                    } else {
                        let poly = &polynomials[k];
                        if poly.degree_bound().is_some() {
                            continue;
                        } else {
                            assert!(poly.degree_bound().is_none());
                            let coeff = E::Fr::rand(rng);
                            value += &(coeff * poly.evaluate(point));
                            lc.add(coeff, label.clone());
                        }
                    }
                }
                values.insert((label.clone(), point), value);
                if !lc.is_empty() {
                    linear_combinations.push(lc);
                    // Insert query
                    query_set.insert((label.clone(), (format!("rand_{i}"), point)));
                }
            }
        }
        if linear_combinations.is_empty() {
            continue;
        }

        let mut sponge_for_open = S::new();
        let proof = SonicKZG10::open_combinations(
            universal_prover,
            &linear_combinations,
            polynomials,
            &rands,
            &query_set,
            &mut sponge_for_open,
        )?;
        println!("Generated proof");
        let mut sponge_for_check = S::new();
        let result = SonicKZG10::check_combinations(
            &universal_verifier,
            &linear_combinations,
            &comms,
            &query_set,
            &values,
            &proof,
            &mut sponge_for_check,
        )?;
        if !result {
            println!("Failed with {num_polynomials} polynomials, num_points_in_query_set: {num_points_in_query_set:?}");
        }
        assert!(result, "proof was incorrect, equations: {linear_combinations:#?}");

        test_components.push(TestComponents {
            universal_verifier,
            commitments: comms,
            query_set,
            evaluations: values,
            batch_lc_proof: Some(proof),
            batch_proof: None,
            randomness: rands,
            _sponge: PhantomData,
        });
    }
    Ok(test_components)
}

pub fn single_poly_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 1,
        enforce_degree_bounds: false,
        max_num_queries: 1,
        ..Default::default()
    };
    test_template::<E, S>(info)
}

pub fn linear_poly_degree_bound_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: Some(2),
        supported_degree: Some(1),
        num_polynomials: 1,
        enforce_degree_bounds: true,
        max_num_queries: 1,
        ..Default::default()
    };
    test_template::<E, S>(info)
}

pub fn single_poly_degree_bound_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 1,
        enforce_degree_bounds: true,
        max_num_queries: 1,
        ..Default::default()
    };
    test_template::<E, S>(info)
}

pub fn quadratic_poly_degree_bound_multiple_queries_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: Some(3),
        supported_degree: Some(2),
        num_polynomials: 1,
        enforce_degree_bounds: true,
        max_num_queries: 2,
        ..Default::default()
    };
    test_template::<E, S>(info)
}

pub fn single_poly_degree_bound_multiple_queries_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 1,
        enforce_degree_bounds: true,
        max_num_queries: 2,
        ..Default::default()
    };
    test_template::<E, S>(info)
}

pub fn two_polys_degree_bound_single_query_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 2,
        enforce_degree_bounds: true,
        max_num_queries: 1,
        ..Default::default()
    };
    test_template::<E, S>(info)
}

pub fn full_end_to_end_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 10,
        enforce_degree_bounds: true,
        max_num_queries: 5,
        ..Default::default()
    };
    test_template::<E, S>(info)
}

pub fn full_end_to_end_equation_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 10,
        enforce_degree_bounds: true,
        max_num_queries: 5,
        num_equations: Some(10),
    };
    equation_test_template::<E, S>(info)
}

pub fn single_equation_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 1,
        enforce_degree_bounds: false,
        max_num_queries: 1,
        num_equations: Some(1),
    };
    equation_test_template::<E, S>(info)
}

pub fn two_equation_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 2,
        enforce_degree_bounds: false,
        max_num_queries: 1,
        num_equations: Some(2),
    };
    equation_test_template::<E, S>(info)
}

pub fn two_equation_degree_bound_test<E, S>() -> Result<Vec<TestComponents<E, S>>, PCError>
where
    E: PairingEngine,
    S: AlgebraicSponge<E::Fq, 2>,
{
    let info = TestInfo {
        num_iters: 100,
        max_degree: None,
        supported_degree: None,
        num_polynomials: 2,
        enforce_degree_bounds: true,
        max_num_queries: 1,
        num_equations: Some(2),
    };
    equation_test_template::<E, S>(info)
}
