//! 試合中に起きた、外へ知らせる価値のある出来事。
//!
//! GameCoreは標準出力へ書かない。通信もOSも持たない層として保つためで、
//! `schedule`の説明もそう宣言している。printしてしまうと次の2つが崩れる。
//!
//! - 学習やリプレイで1秒に何千試合も回すとき、出力そのものが重くなる
//! - 出力先の都合（ファイル、レベル分け、書式）がゲーム計算へ混ざる
//!
//! ここへ積んでおき、外側の層が取り出して好きな形で出す。

use bevy::prelude::Resource;
use std::fmt;

/// 溜め込む上限。
///
/// 外側が取り出さないまま長時間動く場合（GameCore単体の試験など）に、
/// 際限なく増えないようにする。古いものから捨てる。
const CAPACITY: usize = 256;

/// 試合の進行で起きた出来事。
///
/// 文字列ではなく型にしているのは、受け取った側が出し方を選べるようにするため。
/// 「誰が」「何を」が構造のまま残っていれば、後から集計にも使える。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEvent {
    /// 人間が居なくなったので、ルームを空へ戻した。
    RoomReset,
    /// 再接続の猶予が尽きて、プレイヤーを外した。
    ReconnectGraceExpired { player_id: u64 },
    /// 残り1人になったので試合を切り上げた。
    EndedShorthanded,
    /// 切断からの復帰で試合を再開した。
    Resumed,
    /// カウントダウンが終わり、得点を数え始めた。
    Started,
    /// 次の試合の準備が整った（カウントダウン開始）。
    Ready,
    /// 試合が終わった。勝者が居なければ引き分け。
    Finished { winner_id: Option<u64> },
}

impl fmt::Display for MatchEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoomReset => write!(formatter, "room reset because no human players remain"),
            Self::ReconnectGraceExpired { player_id } => {
                write!(formatter, "player {player_id} reconnect grace expired")
            }
            Self::EndedShorthanded => {
                write!(
                    formatter,
                    "match ended because fewer than two players remain"
                )
            }
            Self::Resumed => write!(formatter, "match resumed after reconnect"),
            Self::Started => write!(formatter, "timed score match started"),
            Self::Ready => write!(formatter, "new timed score match is ready"),
            Self::Finished { winner_id } => {
                write!(formatter, "match finished; winner: {winner_id:?}")
            }
        }
    }
}

/// 出来事の置き場。外側の層が毎tick取り出す。
#[derive(Resource, Default)]
pub struct MatchLog {
    events: Vec<MatchEvent>,
    /// 上限を超えて捨てた数。黙って消えると、消えたことにも気付けない。
    dropped: u64,
}

impl MatchLog {
    pub(crate) fn record(&mut self, event: MatchEvent) {
        if self.events.len() >= CAPACITY {
            self.events.remove(0);
            self.dropped += 1;
        }
        self.events.push(event);
    }

    /// 溜まっている出来事を取り出して空にする。
    pub fn drain(&mut self) -> Vec<MatchEvent> {
        std::mem::take(&mut self.events)
    }

    /// 上限を超えて捨てた数を取り出して0へ戻す。
    pub fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draining_empties_the_log() {
        let mut log = MatchLog::default();
        log.record(MatchEvent::Started);
        log.record(MatchEvent::Finished { winner_id: Some(2) });

        assert_eq!(
            log.drain(),
            vec![
                MatchEvent::Started,
                MatchEvent::Finished { winner_id: Some(2) }
            ]
        );
        assert!(log.drain().is_empty(), "取り出した後も残っている");
    }

    /// 誰も取り出さないまま動き続けても、際限なく増えないこと。
    ///
    /// GameCore単体の試験や学習環境では外側が居ない。放っておくと
    /// メモリだけが伸び続ける。
    #[test]
    fn an_unread_log_stops_growing_and_says_how_much_it_dropped() {
        let mut log = MatchLog::default();
        for _ in 0..CAPACITY + 10 {
            log.record(MatchEvent::Started);
        }

        assert_eq!(log.events.len(), CAPACITY);
        assert_eq!(log.take_dropped(), 10, "捨てた数が分からない");
        assert_eq!(log.take_dropped(), 0, "取り出した後も残っている");
    }

    /// 出来事が読める文になること。外側はこれをそのまま出す。
    #[test]
    fn events_read_as_sentences() {
        assert_eq!(
            MatchEvent::ReconnectGraceExpired { player_id: 3 }.to_string(),
            "player 3 reconnect grace expired"
        );
        assert_eq!(
            MatchEvent::Finished { winner_id: None }.to_string(),
            "match finished; winner: None"
        );
    }
}
