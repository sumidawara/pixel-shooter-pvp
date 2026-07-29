//! スコアの飽和加減算。

pub(super) fn add_points(score: i32, points: i32) -> i32 {
    score.saturating_add(points)
}

pub(super) fn subtract_points(score: i32, penalty: i32) -> i32 {
    score.saturating_sub(penalty)
}
