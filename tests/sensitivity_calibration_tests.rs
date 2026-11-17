//! Sensitivity conversion data and the calibration search.

use aim_trainer::sensitivity::{GameDatabase, SensitivityConverter};

fn cm_360(game_id: &str, dpi: f64, sens: f64) -> f64 {
    let db = GameDatabase::new();
    let g = db.get_by_id(game_id).expect("game exists");
    SensitivityConverter::game_sens_to_cm_360(g, dpi, sens)
}

#[test]
fn fortnite_uses_the_in_game_percent_value() {
    // Fortnite's slider shows a percent: 6.3 in game equals 0.5 in Valorant (ratio ~12.6).
    let fortnite = cm_360("fortnite", 800.0, 6.3);
    let valorant = cm_360("valorant", 800.0, 0.5);
    assert!((fortnite - valorant).abs() / valorant < 0.01, "{fortnite} vs {valorant}");
}

// ---------- PSA round decisions ----------

use aim_trainer::sensitivity::path_recorder::ShotTrajectoryPattern::{self, Direct, Overshoot, Undershoot};
use aim_trainer::sensitivity::psa::{PsaCalibrator, PsaRoundConclusion};

fn round(shots: &[(ShotTrajectoryPattern, usize)]) -> PsaRoundConclusion {
    let mut psa = PsaCalibrator::new(15.0, 80.0, 7);
    for &(p, n) in shots {
        for _ in 0..n {
            psa.record_shot(p);
        }
    }
    psa.history.last().expect("a finished round").conclusion
}

#[test]
fn mostly_clean_round_still_follows_the_misses() {
    // Clean shots say nothing about direction; the misses lean one way.
    assert_eq!(round(&[(Direct, 6), (Overshoot, 3), (Undershoot, 1)]), PsaRoundConclusion::TooFastNeedSlower);
    assert_eq!(round(&[(Direct, 6), (Overshoot, 1), (Undershoot, 3)]), PsaRoundConclusion::TooSlowNeedFaster);
}

#[test]
fn even_or_clean_rounds_are_balanced() {
    assert_eq!(round(&[(Direct, 6), (Overshoot, 2), (Undershoot, 2)]), PsaRoundConclusion::BalancedNarrow);
    assert_eq!(round(&[(Direct, 10)]), PsaRoundConclusion::BalancedNarrow);
}

// ---------- Simulated players ----------

use rand::{rngs::StdRng, Rng, SeedableRng};

/// A player whose comfortable sensitivity is `pref` cm/360. Testing faster than that makes
/// misses lean toward overshoot, slower toward undershoot; close to it, most shots are clean.
fn simulated_shot(rng: &mut StdRng, pref: f64, cm: f64) -> ShotTrajectoryPattern {
    simulated_shot_with_clarity(rng, pref, cm, 4.0)
}

/// `clarity`: how strongly the miss direction reacts to a mismatch (higher = clearer signal).
fn simulated_shot_with_clarity(rng: &mut StdRng, pref: f64, cm: f64, clarity: f64) -> ShotTrajectoryPattern {
    let mismatch = (pref / cm).ln();
    let p_direct = 0.45 * (-(mismatch * mismatch) / (2.0 * 0.25f64.powi(2))).exp() + 0.15;
    let p_over_given_miss = 1.0 / (1.0 + (-clarity * mismatch).exp());
    if rng.gen::<f64>() < p_direct {
        Direct
    } else if rng.gen::<f64>() < p_over_given_miss {
        Overshoot
    } else {
        Undershoot
    }
}

fn calibrate(rng: &mut StdRng, pref: f64) -> PsaCalibrator {
    let mut psa = PsaCalibrator::new(15.0, 80.0, 7);
    while !psa.is_completed {
        let cm = psa.current_test_cm_360;
        psa.record_shot(simulated_shot(rng, pref, cm));
    }
    psa
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

#[test]
fn calibration_lands_near_the_preferred_sensitivity() {
    let mut rng = StdRng::seed_from_u64(42);
    let medians: Vec<(f64, f64)> = [20.0, 30.0, 40.0, 55.0, 70.0]
        .into_iter()
        .map(|pref| {
            let errs: Vec<f64> = (0..400)
                .map(|_| {
                    let r = calibrate(&mut rng, pref).recommended_cm_360.unwrap();
                    (r - pref).abs() / pref * 100.0
                })
                .collect();
            (pref, median(errs))
        })
        .collect();
    println!("median error by preferred cm/360: {medians:.1?}");
    assert!(medians.iter().all(|&(_, m)| m <= 8.0), "median errors: {medians:.1?}");
}

// ---------- Psychometric fit over all rounds ----------

use aim_trainer::sensitivity::psa::fit_crossover;

#[test]
fn crossover_sits_where_overshoots_and_undershoots_balance() {
    // (cm/360, overshoots, undershoots): symmetric in log space around sqrt(20 * 45) = 30.
    let fit = fit_crossover(&[(20.0, 9, 1), (30.0, 3, 3), (45.0, 1, 9)]).expect("a crossover");
    assert!((fit.cm_360 - 30.0).abs() < 0.5, "{}", fit.cm_360);
}

#[test]
fn more_evidence_gives_a_tighter_crossover() {
    let few = fit_crossover(&[(20.0, 4, 1), (45.0, 1, 4)]).unwrap();
    let many = fit_crossover(&[(20.0, 40, 10), (45.0, 10, 40)]).unwrap();
    assert!(many.rel_uncertainty < few.rel_uncertainty);
}

#[test]
fn no_crossover_when_misses_dont_change_with_sensitivity() {
    // Undershooting more at faster settings contradicts the model.
    assert!(fit_crossover(&[(20.0, 1, 9), (45.0, 9, 1)]).is_none());
    assert!(fit_crossover(&[]).is_none());
}

// ---------- Full engine: final recommendation and confidence ----------

use aim_trainer::sensitivity::path_recorder::ShotMovementPath;
use aim_trainer::sensitivity::{CalibrationShotOutcome, FinalSensitivityRecommendation, SensitivityEngine};

fn path_with(pattern: ShotTrajectoryPattern) -> ShotMovementPath {
    let now = std::time::Instant::now();
    ShotMovementPath {
        samples: Vec::new(),
        start_time: now,
        end_time: now,
        total_mouse_distance: 300.0,
        peak_velocity: 1500.0,
        direction_changes: 0,
        pattern: Some(pattern),
    }
}

/// Run a whole calibration through the engine. Hits and reaction times also get worse the
/// further the tested sensitivity is from `pref`, as the analyzer expects.
fn engine_run(rng: &mut StdRng, pref: f64, clarity: f64) -> SensitivityEngine {
    let mut e = SensitivityEngine::new(800.0);
    loop {
        let cm = e.psa.current_test_cm_360;
        let m = (cm / pref).ln();
        let hit = rng.gen::<f64>() < 0.85 * (-(m * m) / (2.0 * 0.35f64.powi(2))).exp();
        let rt = 420.0 + 350.0 * m.abs() + rng.gen_range(-80.0..80.0);
        let shot = path_with(simulated_shot_with_clarity(rng, pref, cm, clarity));
        if e.record_calibration_shot(cm, hit, rt, &shot) == CalibrationShotOutcome::Completed {
            return e;
        }
    }
}

fn engine_calibration(rng: &mut StdRng, pref: f64) -> FinalSensitivityRecommendation {
    engine_run(rng, pref, 4.0).compute_final_recommendation()
}

#[test]
fn final_recommendation_is_the_search_result() {
    // The score-curve peak isn't an independent estimate (its samples sit where the search
    // tested), so blending it in only made results worse. Accuracy is covered by
    // `calibration_lands_near_the_preferred_sensitivity`.
    let mut rng = StdRng::seed_from_u64(11);
    for pref in [20.0, 35.0, 55.0] {
        let e = engine_run(&mut rng, pref, 4.0);
        let r = e.compute_final_recommendation();
        assert_eq!(Some(r.optimal_cm_360), e.psa.recommended_cm_360);
        assert_eq!(r.confidence_score, e.psa.confidence());
    }
}

#[test]
fn confidence_is_higher_for_players_whose_misses_say_more() {
    let mut rng = StdRng::seed_from_u64(9);
    let mean_conf = |rng: &mut StdRng, clarity: f64| {
        (0..200).map(|_| engine_run(rng, 35.0, clarity).psa.confidence() as f64).sum::<f64>() / 200.0
    };
    let clear = mean_conf(&mut rng, 8.0);
    let vague = mean_conf(&mut rng, 1.0);
    assert!(clear - vague >= 0.2, "mean confidence: clear signal {clear:.2}, vague signal {vague:.2}");
}

#[test]
fn confidence_is_higher_when_the_result_is_closer() {
    let mut rng = StdRng::seed_from_u64(5);
    let (mut close, mut far) = (Vec::new(), Vec::new());
    for pref in [20.0, 35.0, 55.0] {
        for _ in 0..300 {
            let r = engine_calibration(&mut rng, pref);
            let err = (r.optimal_cm_360 - pref).abs() / pref;
            if err < 0.05 {
                close.push(r.confidence_score as f64);
            } else if err > 0.15 {
                far.push(r.confidence_score as f64);
            }
        }
    }
    let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
    let (c, f) = (mean(&close), mean(&far));
    // A single run's luck can't be predicted from its own data, so this only checks the trend.
    assert!(c > f, "mean confidence: within 5% = {c:.2} (n={}), off by >15% = {f:.2} (n={})", close.len(), far.len());
}

#[test]
fn confidence_is_low_when_the_answer_is_outside_the_search_range() {
    // Wants 10 cm/360; the search only covers 15-80.
    let mut rng = StdRng::seed_from_u64(3);
    let confs: Vec<f64> = (0..100).map(|_| engine_calibration(&mut rng, 10.0).confidence_score as f64).collect();
    let m = median(confs);
    assert!(m < 0.5, "median confidence {m:.2}");
}
