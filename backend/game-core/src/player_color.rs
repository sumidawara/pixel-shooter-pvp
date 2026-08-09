//! 画面上でプレイヤーを見分ける色の割り当て。
//!
//! 以前はクライアントが「参加者をID順に並べた何番目か」で色を決めていた。この決め方は
//! 自分以外の参加者に依存するため、誰かが抜けると残った全員の色が一斉にずれる。
//! 試合中に見分けの手がかりが入れ替わるのは、見た目ではなく操作の問題になる。
//!
//! 色は参加した時点で決まり、本人が変えるまで変わらないものとして、サーバーが持つ。
//! 実際の色そのもの（何番が水色か）はクライアントの持ち物で、ここでは番号しか扱わない。

use pixel_shooter_protocol::player_colors;

/// まだ誰も使っていない色のうち、いちばん若い番号。
///
/// 空きが無ければ0を返す。色の数と参加人数の上限が同じなら空きは必ずあるが、
/// 上限を増やして色を増やし忘れたときに、割り当てが失敗するのではなく
/// 色が重なるだけで済むようにしておく。
pub fn free_color(taken: impl IntoIterator<Item = u8>) -> u8 {
    let mut used = [false; player_colors::COUNT as usize];
    for color in taken {
        if let Some(slot) = used.get_mut(color as usize) {
            *slot = true;
        }
    }
    used.iter()
        .position(|occupied| !occupied)
        .unwrap_or(0)
        .try_into()
        .unwrap_or(0)
}

/// その色へ変えてよいか。
///
/// 範囲外は弾く。既に誰かが使っている色も弾く。入れ替えではなく拒否にするのは、
/// 自分が選んだ結果として他人の色が勝手に変わる方が分かりにくいため。
pub fn can_take(color: u8, taken: impl IntoIterator<Item = u8>) -> bool {
    color < player_colors::COUNT && !taken.into_iter().any(|used| used == color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_player_gets_the_first_color() {
        assert_eq!(free_color([]), 0);
    }

    #[test]
    fn each_player_gets_a_color_nobody_else_has() {
        let mut taken: Vec<u8> = Vec::new();
        for _ in 0..player_colors::COUNT {
            let color = free_color(taken.clone());
            assert!(!taken.contains(&color), "{color} was handed out twice");
            taken.push(color);
        }
    }

    /// 抜けた人の色が次の人へ回ること。
    ///
    /// 埋まっている番号を飛ばすだけだと、抜けた穴が埋まらず色が枯れる。
    #[test]
    fn a_color_freed_by_someone_leaving_is_reused() {
        assert_eq!(free_color([0, 2, 3]), 1);
    }

    #[test]
    fn a_color_someone_else_holds_cannot_be_taken() {
        assert!(!can_take(1, [0, 1]));
        assert!(can_take(2, [0, 1]));
    }

    #[test]
    fn a_color_outside_the_palette_cannot_be_taken() {
        assert!(!can_take(player_colors::COUNT, []));
        assert!(!can_take(200, []));
    }

    /// 自分が今の色を選び直しても弾かれること。
    ///
    /// 呼ぶ側が自分を除いた一覧を渡す前提で、ここは素直に「使われていれば不可」とする。
    #[test]
    fn holding_a_color_blocks_taking_it_again() {
        assert!(!can_take(0, [0]));
    }
}
