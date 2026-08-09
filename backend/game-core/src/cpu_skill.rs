//! CPUの技量を数値で表す。
//!
//! 「強い/弱い」を分けているのは、移動速度や与ダメージのような**ルール**ではなく、
//! 目の届く範囲・反応の速さ・狙いの正確さという**プレイヤー側の能力**だけ。
//! ルールを変えて弱くすると、下手な相手ではなく別のユニットになってしまう。
//!
//! 段階は4つ。3が従来のCPUに相当する強さで、4は弾の到達時間を見越して撃つ。
//!
//! | | 1 | 2 | 3 | 4 |
//! | --- | --- | --- | --- | --- |
//! | 視界 | 狭い | やや狭い | プレイヤーと同程度 | 広い |
//! | 反応 | 0.50秒 | 0.30秒 | 0.13秒 | 即時 |
//! | 狙い | 遅く雑 | やや遅い | 速い | 即時・偏差撃ち |
//! | 退避 | しない | 近づかれたら | 従来と同じ | 距離を保つ |
//!
//! 数値は`server.json`の`cpu.levels`から上書きできる。手触りの調整は
//! 何度も試すことになるので、その都度ビルドし直さずに済むようにしてある。

use serde::Deserialize;

/// CPUの強さの段階。ロビーから選ぶ。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum CpuLevel {
    /// 初めて遊ぶ人が勝てる強さ。視界が狭く、反応も狙いも遅い。
    One,
    Two,
    /// 従来のCPUに相当する強さ。既定はここ。
    #[default]
    Three,
    /// 弾の到達時間を見越して撃つ。
    Four,
}

/// 段階の数。ロビーの選択肢や設定配列の長さがこれに揃う。
pub const CPU_LEVEL_COUNT: usize = 4;

/// どの段階のCPUも、これより遠くは見ない。
///
/// プレイヤーがカメラで見渡せる距離を超えさせないための上限。ただし
/// 「カメラでどこまで見えるか」を決めるのはクライアントの寄り具合(FOLLOW_ZOOM)で、
/// こちら側からは分からない。**この値を守っているかは
/// `frontend/tests/game_view_test.gd` が画面側で検査する。**
/// 片側だけで数値を持つと、寄り具合を変えたときに前提だけが静かに古くなる。
pub const MAX_SIGHT_RADIUS: f32 = 250.0;

impl CpuLevel {
    /// 1〜4の数から段階を作る。範囲外は近い方へ寄せる。
    ///
    /// 通信や設定ファイルから来る値をそのまま受けるので、弾かずに丸める。
    pub fn from_number(number: u8) -> Self {
        match number {
            0 | 1 => Self::One,
            2 => Self::Two,
            3 => Self::Three,
            _ => Self::Four,
        }
    }

    pub fn number(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
        }
    }

    fn index(self) -> usize {
        self.number() as usize - 1
    }
}

/// CPU1体分の能力。すべて「プレイヤーとしての上手さ」に対応する。
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct CpuSkill {
    /// 見える距離(px)。これより遠い相手やアイテムは、居ないものとして動く。
    ///
    /// 壁の向こうは段階によらず見えない。位置を透視して動くCPUは、
    /// 何をされたのか分からないまま撃たれるので、強い弱い以前に理不尽になる。
    pub sight_radius: f32,
    /// 見え始めてから動き出すまでのtick数。狙いも同じだけ遅れて追いつく。
    ///
    /// 60tick=1秒。人の反応はおおよそ12〜15tick。
    pub reaction_ticks: u64,
    /// 狙いを動かせる速さ(度/秒)。遅いほど、横へ動く相手を追い切れない。
    pub aim_turn_degrees: f32,
    /// 狙いのぶれ幅(度)。速い乱数ではなく、ゆっくり漂う。
    ///
    /// 毎tickの乱数だと平均すると中心に戻るため、結局当たってしまう。
    pub aim_drift_degrees: f32,
    /// 当たる向きから、さらにどれだけずれていても撃つか(度)。
    ///
    /// 「当たる向き」は距離から決まる（近いほど広い）。ここはその外側の余裕で、
    /// 大きいほど雑に撃つ。外れるだけでなく弾を切らしてリロードに入るので、
    /// 見た目にも「焦っている下手な相手」になる。
    ///
    /// 固定の角度にしないのは、近距離だと厳しすぎて上手いCPUほど撃てなくなるため。
    pub fire_cone_degrees: f32,
    /// 相手へ近づくとき、横へ回り込む割合(0〜1)。
    ///
    /// まっすぐ寄ると、相手から見た自分の角速度がほぼ0になり、相手は狙いを
    /// 動かさなくても当てられる。横へ動くほど相手の旋回速度を要求するので、
    /// 撃たれにくくなると同時に、相手の`aim_turn_degrees`の低さを突ける。
    pub strafe_bias: f32,
    /// 弾の到達時間を見越して、相手の進む先を撃つか。
    pub leads_target: bool,
    /// 退避を始める距離(タイル)。0なら退避しない。
    pub retreat_start_tiles: f32,
    /// 見えたアイテムを取りに行くか。
    pub seeks_items: bool,
}

impl Default for CpuSkill {
    /// 基準は段階3。設定ファイルで一部だけ書いたとき、残りはここから埋まる。
    fn default() -> Self {
        Self::preset(CpuLevel::Three)
    }
}

impl CpuSkill {
    /// 段階ごとの組み込みの値。
    ///
    /// 視界の基準は、プレイヤーがカメラで見渡せる範囲。段階3の200pxが
    /// 「プレイヤーと同程度、ただし全周」に当たる。上限は[`MAX_SIGHT_RADIUS`]で、
    /// それがプレイヤーの視界に収まっているかは画面側が検査する。
    pub fn preset(level: CpuLevel) -> Self {
        match level {
            CpuLevel::One => Self {
                sight_radius: 110.0,
                reaction_ticks: 30,
                aim_turn_degrees: 90.0,
                aim_drift_degrees: 14.0,
                fire_cone_degrees: 35.0,
                strafe_bias: 0.0,
                leads_target: false,
                retreat_start_tiles: 0.0,
                seeks_items: false,
            },
            CpuLevel::Two => Self {
                sight_radius: 155.0,
                reaction_ticks: 18,
                aim_turn_degrees: 180.0,
                aim_drift_degrees: 7.0,
                fire_cone_degrees: 22.0,
                strafe_bias: 0.3,
                leads_target: false,
                retreat_start_tiles: 1.2,
                seeks_items: true,
            },
            CpuLevel::Three => Self {
                sight_radius: 200.0,
                reaction_ticks: 8,
                aim_turn_degrees: 540.0,
                aim_drift_degrees: 2.5,
                fire_cone_degrees: 12.0,
                strafe_bias: 0.55,
                leads_target: false,
                retreat_start_tiles: 1.75,
                seeks_items: true,
            },
            CpuLevel::Four => Self {
                sight_radius: 250.0,
                reaction_ticks: 0,
                aim_turn_degrees: 1800.0,
                aim_drift_degrees: 0.0,
                fire_cone_degrees: 4.0,
                strafe_bias: 0.75,
                leads_target: true,
                retreat_start_tiles: 2.4,
                seeks_items: true,
            },
        }
    }

    /// 設定ファイルの書き間違いで、計算が破綻しない範囲へ収める。
    pub fn sanitize(&mut self) {
        self.sight_radius = self.sight_radius.clamp(0.0, 4096.0);
        self.reaction_ticks = self.reaction_ticks.min(600);
        // 0にすると狙いが永久に動かず、その場に固まって見える。
        self.aim_turn_degrees = self.aim_turn_degrees.clamp(1.0, 100_000.0);
        self.aim_drift_degrees = self.aim_drift_degrees.clamp(0.0, 90.0);
        self.fire_cone_degrees = self.fire_cone_degrees.clamp(0.0, 180.0);
        // 1.0にすると真横だけを向いて永久に近づかない。
        self.strafe_bias = self.strafe_bias.clamp(0.0, 0.9);
        self.retreat_start_tiles = self.retreat_start_tiles.clamp(0.0, 20.0);
    }
}

/// 段階ごとの設定。`server.json`の`cpu`に対応する。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CpuSettings {
    /// 段階1から順に並べる。書かれていない段階は組み込みの値を使う。
    pub levels: Vec<CpuSkill>,
}

impl CpuSettings {
    pub fn skill(&self, level: CpuLevel) -> CpuSkill {
        self.levels
            .get(level.index())
            .copied()
            .unwrap_or_else(|| CpuSkill::preset(level))
    }

    pub fn sanitize(&mut self) {
        self.levels.truncate(CPU_LEVEL_COUNT);
        for skill in &mut self.levels {
            skill.sanitize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_outside_the_range_land_on_the_nearest_level() {
        assert_eq!(CpuLevel::from_number(0), CpuLevel::One);
        assert_eq!(CpuLevel::from_number(1), CpuLevel::One);
        assert_eq!(CpuLevel::from_number(4), CpuLevel::Four);
        assert_eq!(CpuLevel::from_number(9), CpuLevel::Four);
        for number in 1..=4u8 {
            assert_eq!(CpuLevel::from_number(number).number(), number);
        }
    }

    /// 段階が上がるほど、すべての能力が上がるか同じであること。
    ///
    /// 1つでも逆行していると、番号の大きい方が弱い場面ができて、
    /// 「段階」として選べなくなる。
    #[test]
    fn every_ability_improves_with_the_level() {
        let levels = [
            CpuLevel::One,
            CpuLevel::Two,
            CpuLevel::Three,
            CpuLevel::Four,
        ];
        for pair in levels.windows(2) {
            let low = CpuSkill::preset(pair[0]);
            let high = CpuSkill::preset(pair[1]);
            let label = format!("{:?} -> {:?}", pair[0], pair[1]);
            assert!(high.sight_radius > low.sight_radius, "{label}: 視界");
            assert!(high.reaction_ticks < low.reaction_ticks, "{label}: 反応");
            assert!(
                high.aim_turn_degrees > low.aim_turn_degrees,
                "{label}: 旋回"
            );
            assert!(
                high.aim_drift_degrees < low.aim_drift_degrees,
                "{label}: ぶれ"
            );
            assert!(
                high.fire_cone_degrees < low.fire_cone_degrees,
                "{label}: 発砲"
            );
            assert!(high.strafe_bias > low.strafe_bias, "{label}: 回り込み");
            assert!(high.leads_target >= low.leads_target, "{label}: 偏差撃ち");
            assert!(high.seeks_items >= low.seeks_items, "{label}: アイテム");
        }
    }

    /// 偏差撃ちは最上段だけであること。下の段でも当たると、段階差が消える。
    #[test]
    fn only_the_top_level_leads_its_shots() {
        assert!(CpuSkill::preset(CpuLevel::Four).leads_target);
        for level in [CpuLevel::One, CpuLevel::Two, CpuLevel::Three] {
            assert!(!CpuSkill::preset(level).leads_target, "{level:?}");
        }
    }

    /// どの段階も、宣言した上限を超えて見ないこと。
    ///
    /// 上限とプレイヤーの視界の関係は、寄り具合を持っている画面側が検査する
    /// （`frontend/tests/game_view_test.gd`）。こちらは「宣言した値を守る」
    /// ことだけを見る。画面の定数をこちらへ書き写すと、あちらを変えたときに
    /// この前提だけが静かに古くなる。
    #[test]
    fn no_level_sees_further_than_the_declared_limit() {
        for number in 1..=4u8 {
            let skill = CpuSkill::preset(CpuLevel::from_number(number));
            assert!(
                skill.sight_radius <= MAX_SIGHT_RADIUS,
                "段階{number}の視界が上限を超えている: {} > {MAX_SIGHT_RADIUS}",
                skill.sight_radius
            );
        }
        // 上限そのものが使われずに浮いていないこと。
        assert!(
            (1..=4u8).any(|number| {
                CpuSkill::preset(CpuLevel::from_number(number)).sight_radius == MAX_SIGHT_RADIUS
            }),
            "どの段階も上限に届いていない。上限が実態と離れている"
        );
    }

    /// 設定ファイルから来た極端な値を丸めること。
    #[test]
    fn broken_settings_are_clamped_instead_of_breaking_the_maths() {
        let mut skill = CpuSkill {
            sight_radius: -50.0,
            reaction_ticks: 100_000,
            aim_turn_degrees: 0.0,
            aim_drift_degrees: 720.0,
            fire_cone_degrees: -5.0,
            strafe_bias: 5.0,
            leads_target: true,
            retreat_start_tiles: 900.0,
            seeks_items: true,
        };

        skill.sanitize();

        assert_eq!(skill.sight_radius, 0.0);
        assert_eq!(skill.reaction_ticks, 600);
        assert!(skill.aim_turn_degrees > 0.0, "狙いが永久に動かなくなる");
        assert_eq!(skill.aim_drift_degrees, 90.0);
        assert_eq!(skill.fire_cone_degrees, 0.0);
        assert_eq!(skill.strafe_bias, 0.9, "真横だけを向いて近づかなくなる");
        assert_eq!(skill.retreat_start_tiles, 20.0);
    }

    /// 設定を書かなければ組み込みの値、書けばそちらを使うこと。
    #[test]
    fn settings_fall_back_to_the_built_in_values() {
        let mut settings = CpuSettings::default();
        assert_eq!(
            settings.skill(CpuLevel::One),
            CpuSkill::preset(CpuLevel::One)
        );

        let mut custom = CpuSkill::preset(CpuLevel::One);
        custom.sight_radius = 64.0;
        settings.levels = vec![custom];

        assert_eq!(settings.skill(CpuLevel::One).sight_radius, 64.0);
        // 書かれていない段階は組み込みのまま。
        assert_eq!(
            settings.skill(CpuLevel::Four),
            CpuSkill::preset(CpuLevel::Four)
        );
    }
}
