use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::habits::analysis::{Dir, Habit, HabitFinding};
use aim_trainer::habits::trend::Progress;
use aim_trainer::habits::LeftOutNote;
use aim_trainer::ui::habits::{card_lines, left_out_sentence};

fn finding(habit: Habit, value: f32, direction: Option<Dir>) -> HabitFinding {
    HabitFinding { habit, value, direction }
}
fn far_short(direction: Option<Dir>) -> HabitFinding {
    finding(Habit::FlickShort(FlickBand::Far), 0.82, direction)
}

#[test]
fn card_reads_like_the_spec() {
    let l = card_lines(
        &far_short(Some(Dir::Left)),
        &Progress { points: vec![0.85, 0.88], practising_elsewhere: false },
    );
    assert_eq!(l.title, "Far flicks stop short");
    assert_eq!(l.value, "18% short");
    assert_eq!(
        l.why,
        "Still there at your best sensitivity → practise, don't change it. Mostly to the left."
    );
    assert_eq!(l.progress.as_deref(), Some("Since calibrating: 18% → 12%"));
    assert_eq!(l.button, "Practise: Flick");
}

#[test]
fn direction_sentences() {
    let why = |d| {
        card_lines(&far_short(d), &Progress { points: vec![], practising_elsewhere: false }).why
    };
    let base = "Still there at your best sensitivity → practise, don't change it.";
    assert_eq!(why(None), base);
    assert_eq!(why(Some(Dir::Right)), format!("{base} Mostly to the right."));
    assert_eq!(why(Some(Dir::Up)), format!("{base} Mostly upward."));
    assert_eq!(why(Some(Dir::Down)), format!("{base} Mostly downward."));
    assert_eq!(why(Some(Dir::SideToSide)), format!("{base} Mostly side to side."));
    assert_eq!(why(Some(Dir::UpAndDown)), format!("{base} Mostly up and down."));
}

#[test]
fn button_uses_practice_screen_names() {
    let p = Progress { points: vec![], practising_elsewhere: false };
    assert_eq!(card_lines(&finding(Habit::MicroCreep, 0.3, None), &p).button, "Practise: Micro-adjust");
    assert_eq!(card_lines(&finding(Habit::TrackingTrails, 60.0, None), &p).button, "Practise: Tracking");
}

#[test]
fn elsewhere_line() {
    let l = card_lines(&far_short(None), &Progress { points: vec![], practising_elsewhere: true });
    assert_eq!(l.progress.as_deref(), Some("Practise at this sensitivity to track progress."));
}

#[test]
fn no_progress_line_without_sessions() {
    let l = card_lines(&far_short(None), &Progress { points: vec![], practising_elsewhere: false });
    assert_eq!(l.progress, None);
}

#[test]
fn since_line_keeps_the_unit_out_of_the_numbers() {
    let l = card_lines(
        &finding(Habit::TrackingTrails, 62.0, None),
        &Progress { points: vec![48.0], practising_elsewhere: false },
    );
    assert_eq!(l.progress.as_deref(), Some("Since calibrating: 62 → 48"));
}

#[test]
fn left_out_sentence_text() {
    let n = LeftOutNote { test: "Tracking".into(), habit: Habit::TrackingTrails };
    assert_eq!(
        left_out_sentence(&n),
        "Tracking was left out of the result: you trailed the target at every speed tested. That's a habit to practise, not a sensitivity problem."
    );
}
