//! The paired comparison core: sides, ABBA order, judging a pair, pair bookkeeping.

use aim_trainer::sensitivity::combiner::Signal;
use aim_trainer::sensitivity::paired::{abba, judge, PairBook, PairResult, Side, STEP};

use Side::{Faster as F, Slower as S};

#[test]
fn sides_scale_by_step() {
    assert!((F.factor() - (1.0 / 1.12)).abs() < 1e-6);
    assert!((S.factor() - 1.12).abs() < 1e-6);
    assert_eq!(STEP, 1.12);
    assert_eq!(F.other(), S);
    assert_eq!(S.other(), F);
}

#[test]
fn abba_order() {
    assert_eq!(abba(F, 8), vec![F, S, S, F, F, S, S, F]);
    assert_eq!(abba(S, 4), vec![S, F, F, S]);
    assert_eq!(abba(F, 3).len(), 3);
}

#[test]
fn judge_directions() {
    assert_eq!(judge(1.0, 0.8), Signal::TooFast);
    assert_eq!(judge(0.8, 1.0), Signal::TooSlow);
    assert_eq!(judge(1.0, 0.98), Signal::Direct, "2% is within the 3% tie");
    assert_eq!(judge(1.0, 0.95), Signal::TooFast);
}

#[test]
fn pair_book_pairs_slots_in_order() {
    let mut b = PairBook::new(abba(F, 4));
    b.record(0, Some(1.0));
    assert!(b.take_finished().is_empty(), "half a pair");
    b.record(1, Some(2.0));
    assert_eq!(
        b.take_finished(),
        vec![PairResult { faster_cost: 1.0, slower_cost: 2.0, signal: Signal::TooSlow, band: None }]
    );
    assert!(b.take_finished().is_empty(), "each pair once");
}

#[test]
fn pair_book_maps_costs_by_side() {
    let mut b = PairBook::new(abba(S, 2));
    assert_eq!(b.side(0), Some(S));
    assert_eq!(b.side(2), None);
    b.record(0, Some(1.0));
    b.record(1, Some(2.0));
    let r = b.take_finished();
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].faster_cost, r[0].slower_cost, r[0].signal), (2.0, 1.0, Signal::TooFast));
}

#[test]
fn dropped_slot_drops_its_pair_only() {
    let mut b = PairBook::new(abba(F, 4));
    b.record(0, None);
    b.record(1, Some(1.0));
    b.record(2, Some(1.0));
    b.record(3, Some(1.5));
    let r = b.take_finished();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].faster_cost, 1.5, "slot 3 is the faster side in A B B A with A = faster");
}
