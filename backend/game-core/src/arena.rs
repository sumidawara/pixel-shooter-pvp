//! GameCoreのデータ駆動アリーナ、衝突判定、スポーン地点。

use std::{fmt, fs, path::Path};

use bevy::prelude::{Resource, Vec2};
use pixel_shooter_protocol::{MapDefinition, PLAYER_RADIUS};
use serde::Serialize;

use crate::model::MAX_PLAYERS;

const CLASSIC_ARENA_JSON: &str = include_str!("../../../frontend/maps/classic_arena.json");
const MAX_MAP_WIDTH: usize = 256;
const MAX_MAP_HEIGHT: usize = 256;
const MIN_TILE_SIZE: u32 = 8;
const MAX_TILE_SIZE: u32 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TileKind {
    Floor,
    SolidWall,
    DestructibleWall,
}

impl TileKind {
    pub fn is_obstacle(self) -> bool {
        matches!(self, Self::SolidWall | Self::DestructibleWall)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridPosition {
    pub x: usize,
    pub y: usize,
}

/// 検証済みのマップ。ゲーム中は文字列ではなくこのResourceだけを参照する。
#[derive(Resource, Clone, Debug)]
pub struct ArenaMap {
    id: String,
    revision: String,
    name: String,
    width: usize,
    height: usize,
    tile_size: f32,
    tiles: Vec<TileKind>,
    spawn_points: Vec<GridPosition>,
    item_spawn_points: Vec<GridPosition>,
}

#[derive(Debug)]
pub enum MapLoadError {
    Read(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for MapLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "could not read map: {error}"),
            Self::Json(error) => write!(formatter, "could not parse map JSON: {error}"),
            Self::Invalid(error) => write!(formatter, "invalid map: {error}"),
        }
    }
}

impl std::error::Error for MapLoadError {}

impl ArenaMap {
    pub fn from_json(json: &str) -> Result<Self, MapLoadError> {
        let definition: MapDefinition = serde_json::from_str(json).map_err(MapLoadError::Json)?;
        Self::validate(definition)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, MapLoadError> {
        let json = fs::read_to_string(path).map_err(MapLoadError::Read)?;
        Self::from_json(&json)
    }

    fn validate(definition: MapDefinition) -> Result<Self, MapLoadError> {
        if definition.schema_version != 1 {
            return Err(MapLoadError::Invalid(format!(
                "unsupported schema_version {}; expected 1",
                definition.schema_version
            )));
        }
        if definition.id.trim().is_empty() {
            return Err(MapLoadError::Invalid("id must not be empty".into()));
        }
        if definition.revision.trim().is_empty() {
            return Err(MapLoadError::Invalid("revision must not be empty".into()));
        }
        if definition.width == 0
            || definition.width > MAX_MAP_WIDTH
            || definition.height == 0
            || definition.height > MAX_MAP_HEIGHT
        {
            return Err(MapLoadError::Invalid(format!(
                "width and height must be within 1..={MAX_MAP_WIDTH} and 1..={MAX_MAP_HEIGHT}"
            )));
        }
        if !(MIN_TILE_SIZE..=MAX_TILE_SIZE).contains(&definition.tile_size) {
            return Err(MapLoadError::Invalid(format!(
                "tile_size must be within {MIN_TILE_SIZE}..={MAX_TILE_SIZE}"
            )));
        }
        if definition.tiles.len() != definition.height {
            return Err(MapLoadError::Invalid(format!(
                "tiles has {} rows; expected {}",
                definition.tiles.len(),
                definition.height
            )));
        }

        let mut tiles = Vec::with_capacity(definition.width * definition.height);
        for (y, row) in definition.tiles.iter().enumerate() {
            let characters: Vec<char> = row.chars().collect();
            if characters.len() != definition.width {
                return Err(MapLoadError::Invalid(format!(
                    "row {y} has {} tiles; expected {}",
                    characters.len(),
                    definition.width
                )));
            }
            for (x, character) in characters.into_iter().enumerate() {
                tiles.push(match character {
                    '.' => TileKind::Floor,
                    '#' => TileKind::SolidWall,
                    'X' => TileKind::DestructibleWall,
                    _ => {
                        return Err(MapLoadError::Invalid(format!(
                            "unknown tile '{character}' at ({x}, {y})"
                        )));
                    }
                });
            }
        }

        let spawn_points = definition
            .spawn_points
            .into_iter()
            .map(|[x, y]| GridPosition { x, y })
            .collect();
        let item_spawn_points = definition
            .item_spawn_points
            .into_iter()
            .map(|[x, y]| GridPosition { x, y })
            .collect();
        let map = Self {
            id: definition.id,
            revision: definition.revision,
            name: definition.name,
            width: definition.width,
            height: definition.height,
            tile_size: definition.tile_size as f32,
            tiles,
            spawn_points,
            item_spawn_points,
        };
        map.validate_points("spawn point", &map.spawn_points, PLAYER_RADIUS)?;
        map.validate_points("item spawn point", &map.item_spawn_points, 0.0)?;
        if map.spawn_points.len() < MAX_PLAYERS {
            return Err(MapLoadError::Invalid(format!(
                "map has {} spawn points; at least {MAX_PLAYERS} are required",
                map.spawn_points.len()
            )));
        }
        if map.item_spawn_points.is_empty() {
            return Err(MapLoadError::Invalid(
                "at least one item spawn point is required".into(),
            ));
        }
        Ok(map)
    }

    fn validate_points(
        &self,
        label: &str,
        points: &[GridPosition],
        margin: f32,
    ) -> Result<(), MapLoadError> {
        for point in points {
            if point.x >= self.width || point.y >= self.height {
                return Err(MapLoadError::Invalid(format!(
                    "{label} ({}, {}) is outside the map",
                    point.x, point.y
                )));
            }
            if self.obstacle_at(self.tile_center(*point), margin) {
                return Err(MapLoadError::Invalid(format!(
                    "{label} ({}, {}) overlaps a wall",
                    point.x, point.y
                )));
            }
        }
        Ok(())
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn tile_size(&self) -> f32 {
        self.tile_size
    }

    pub fn pixel_width(&self) -> f32 {
        self.width as f32 * self.tile_size
    }

    pub fn pixel_height(&self) -> f32 {
        self.height as f32 * self.tile_size
    }

    pub fn tile(&self, position: GridPosition) -> Option<TileKind> {
        (position.x < self.width && position.y < self.height)
            .then(|| self.tiles[position.y * self.width + position.x])
    }

    pub fn tile_center(&self, position: GridPosition) -> Vec2 {
        Vec2::new(
            (position.x as f32 + 0.5) * self.tile_size,
            (position.y as f32 + 0.5) * self.tile_size,
        )
    }

    pub fn spawn_position(&self, index: usize) -> Vec2 {
        self.tile_center(self.spawn_points[index % self.spawn_points.len()])
    }

    pub fn item_spawn_position(&self, index: usize) -> Vec2 {
        self.tile_center(self.item_spawn_points[index % self.item_spawn_points.len()])
    }

    pub fn item_spawn_count(&self) -> usize {
        self.item_spawn_points.len()
    }

    /// 接続時にGodotへ一度だけ送る、検証済みマップの通信表現。
    pub fn definition(&self) -> MapDefinition {
        let tiles = self
            .tiles
            .chunks(self.width)
            .map(|row| {
                row.iter()
                    .map(|tile| match tile {
                        TileKind::Floor => '.',
                        TileKind::SolidWall => '#',
                        TileKind::DestructibleWall => 'X',
                    })
                    .collect()
            })
            .collect();
        MapDefinition {
            schema_version: 1,
            id: self.id.clone(),
            revision: self.revision.clone(),
            name: self.name.clone(),
            width: self.width,
            height: self.height,
            tile_size: self.tile_size as u32,
            tiles,
            spawn_points: self
                .spawn_points
                .iter()
                .map(|point| [point.x, point.y])
                .collect(),
            item_spawn_points: self
                .item_spawn_points
                .iter()
                .map(|point| [point.x, point.y])
                .collect(),
        }
    }

    pub fn move_with_collision(&self, position: &mut Vec2, delta: Vec2) {
        let mut next = *position;
        next.x += delta.x;
        if self.valid_player_position(next) {
            position.x = next.x;
        }
        next = *position;
        next.y += delta.y;
        if self.valid_player_position(next) {
            position.y = next.y;
        }
    }

    pub fn valid_player_position(&self, position: Vec2) -> bool {
        position.x >= PLAYER_RADIUS
            && position.x <= self.pixel_width() - PLAYER_RADIUS
            && position.y >= PLAYER_RADIUS
            && position.y <= self.pixel_height() - PLAYER_RADIUS
            && !self.obstacle_at(position, PLAYER_RADIUS)
    }

    pub fn bullet_in_bounds(&self, position: Vec2) -> bool {
        position.x >= 0.0
            && position.x <= self.pixel_width()
            && position.y >= 0.0
            && position.y <= self.pixel_height()
    }

    /// 点の周囲をmarginだけ広げた矩形が、壁タイルと交差するか判定する。
    pub fn obstacle_at(&self, position: Vec2, margin: f32) -> bool {
        if position.x + margin < 0.0
            || position.y + margin < 0.0
            || position.x - margin > self.pixel_width()
            || position.y - margin > self.pixel_height()
        {
            return false;
        }
        let min_x = ((position.x - margin).max(0.0) / self.tile_size).floor() as usize;
        let min_y = ((position.y - margin).max(0.0) / self.tile_size).floor() as usize;
        let max_x = ((position.x + margin).max(0.0) / self.tile_size).floor() as usize;
        let max_y = ((position.y + margin).max(0.0) / self.tile_size).floor() as usize;
        for y in min_y..=max_y.min(self.height.saturating_sub(1)) {
            for x in min_x..=max_x.min(self.width.saturating_sub(1)) {
                if self
                    .tile(GridPosition { x, y })
                    .is_some_and(TileKind::is_obstacle)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn choose_respawn_position(
        &self,
        player_id: u64,
        player_positions: &[(u64, Vec2)],
        bullet_positions: &[Vec2],
        tick: u64,
    ) -> Vec2 {
        self.spawn_points
            .iter()
            .enumerate()
            .map(|(index, point)| (index, self.tile_center(*point)))
            .filter(|(_, candidate)| self.valid_player_position(*candidate))
            .max_by(|(left_index, left), (right_index, right)| {
                let left_score = self.respawn_safety_score(
                    player_id,
                    *left,
                    *left_index,
                    player_positions,
                    bullet_positions,
                    tick,
                );
                let right_score = self.respawn_safety_score(
                    player_id,
                    *right,
                    *right_index,
                    player_positions,
                    bullet_positions,
                    tick,
                );
                left_score.total_cmp(&right_score)
            })
            .map(|(_, position)| position)
            .unwrap_or_else(|| self.spawn_position((player_id as usize) % MAX_PLAYERS))
    }

    fn respawn_safety_score(
        &self,
        player_id: u64,
        candidate: Vec2,
        candidate_index: usize,
        player_positions: &[(u64, Vec2)],
        bullet_positions: &[Vec2],
        tick: u64,
    ) -> f32 {
        let opponent_distance = player_positions
            .iter()
            .filter(|(id, _)| *id != player_id)
            .map(|(_, position)| candidate.distance(*position))
            .fold(self.pixel_width(), f32::min);
        let bullet_distance = bullet_positions
            .iter()
            .map(|position| candidate.distance(*position))
            .fold(self.pixel_width(), f32::min);
        let variation = ((tick + player_id * 31 + candidate_index as u64 * 17) % 11) as f32;
        opponent_distance + bullet_distance * 0.25 + variation
    }
}

impl Default for ArenaMap {
    fn default() -> Self {
        Self::from_json(CLASSIC_ARENA_JSON).expect("embedded classic arena must be valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_map_is_valid() {
        let map = ArenaMap::default();
        assert_eq!(map.id(), "classic_arena");
        assert_eq!(map.pixel_width(), 640.0);
        assert_eq!(map.pixel_height(), 352.0);
    }

    #[test]
    fn network_definition_round_trips_to_the_same_map() {
        let map = ArenaMap::default();
        let json = serde_json::to_string(&map.definition()).expect("serialize map definition");
        let restored = ArenaMap::from_json(&json).expect("deserialize map definition");
        assert_eq!(restored.id(), map.id());
        assert_eq!(restored.revision(), map.revision());
        assert_eq!(restored.width(), map.width());
        assert_eq!(restored.height(), map.height());
        assert_eq!(
            restored.spawn_position(0),
            map.spawn_position(0),
            "spawn coordinates must survive transport"
        );
    }

    #[test]
    fn invalid_row_width_is_rejected() {
        let json =
            CLASSIC_ARENA_JSON.replacen("\"####################\"", "\"###################\"", 1);
        assert!(matches!(
            ArenaMap::from_json(&json),
            Err(MapLoadError::Invalid(_))
        ));
    }

    #[test]
    fn collision_keeps_player_outside_wall_tile() {
        let map = ArenaMap::default();
        let mut position = Vec2::new(143.0, 80.0);
        map.move_with_collision(&mut position, Vec2::new(10.0, 0.0));
        assert_eq!(position, Vec2::new(143.0, 80.0));
    }

    #[test]
    fn respawn_prefers_the_side_away_from_opponent() {
        let map = ArenaMap::default();
        let position = map.choose_respawn_position(1, &[(2, Vec2::new(60.0, 180.0))], &[], 100);
        assert!(position.x > map.pixel_width() * 0.5);
    }
}
