//! タイル単位の経路探索と、プレイヤーが直進できる区間の判定。

use std::{cmp::Reverse, collections::BinaryHeap};

use bevy::prelude::Vec2;

use super::{ArenaMap, GridPosition};

impl ArenaMap {
    pub fn grid_position(&self, position: Vec2) -> Option<GridPosition> {
        if position.x < 0.0
            || position.y < 0.0
            || position.x >= self.pixel_width()
            || position.y >= self.pixel_height()
        {
            return None;
        }
        Some(GridPosition {
            x: (position.x / self.tile_size).floor() as usize,
            y: (position.y / self.tile_size).floor() as usize,
        })
    }

    /// プレイヤーの当たり判定を含め、2点間を直進できるかを調べる。
    pub(crate) fn has_clear_player_path(&self, start: Vec2, end: Vec2) -> bool {
        if !self.valid_player_position(start) || !self.valid_player_position(end) {
            return false;
        }
        let distance = start.distance(end);
        let sample_distance = (self.tile_size * 0.25).max(1.0);
        let steps = (distance / sample_distance).ceil().max(1.0) as usize;
        (0..=steps).all(|step| {
            let position = start.lerp(end, step as f32 / steps as f32);
            self.valid_player_position(position)
        })
    }

    /// A*で壁を避ける経路を求め、開始地点を除くウェイポイント列へ変換する。
    pub(crate) fn find_player_path(&self, start: Vec2, end: Vec2) -> Option<Vec<Vec2>> {
        let start_cell = self.grid_position(start)?;
        let goal_cell = self.grid_position(end)?;
        if !self.valid_player_position(self.tile_center(goal_cell)) {
            return None;
        }
        if start_cell == goal_cell {
            return Some(vec![end]);
        }

        let cell_count = self.width * self.height;
        let mut costs = vec![usize::MAX; cell_count];
        let mut previous = vec![None; cell_count];
        let mut open = BinaryHeap::new();
        costs[self.tile_index(start_cell)] = 0;
        let initial_distance = Self::grid_distance(start_cell, goal_cell);
        open.push(Reverse((initial_distance, initial_distance, 0, start_cell)));

        while let Some(Reverse((_, _, cost, current))) = open.pop() {
            if cost != costs[self.tile_index(current)] {
                continue;
            }
            if current == goal_cell {
                return self.reconstruct_path(start_cell, current, end, &previous);
            }

            for neighbor in self.neighbors(current).into_iter().flatten() {
                if !self.valid_player_position(self.tile_center(neighbor)) {
                    continue;
                }
                let tentative_cost = cost + 1;
                let neighbor_index = self.tile_index(neighbor);
                if tentative_cost >= costs[neighbor_index] {
                    continue;
                }
                costs[neighbor_index] = tentative_cost;
                previous[neighbor_index] = Some(current);
                let distance = Self::grid_distance(neighbor, goal_cell);
                open.push(Reverse((
                    tentative_cost + distance,
                    distance,
                    tentative_cost,
                    neighbor,
                )));
            }
        }
        None
    }

    fn reconstruct_path(
        &self,
        start: GridPosition,
        goal: GridPosition,
        end: Vec2,
        previous: &[Option<GridPosition>],
    ) -> Option<Vec<Vec2>> {
        let mut cells = vec![goal];
        let mut cursor = goal;
        while cursor != start {
            cursor = previous[self.tile_index(cursor)]?;
            cells.push(cursor);
        }
        cells.reverse();
        let mut waypoints: Vec<Vec2> = cells
            .into_iter()
            .skip(1)
            .map(|cell| self.tile_center(cell))
            .collect();
        if waypoints
            .last()
            .is_none_or(|position| position.distance_squared(end) > 1.0)
        {
            waypoints.push(end);
        }
        Some(waypoints)
    }

    fn tile_index(&self, position: GridPosition) -> usize {
        position.y * self.width + position.x
    }

    fn grid_distance(left: GridPosition, right: GridPosition) -> usize {
        left.x.abs_diff(right.x) + left.y.abs_diff(right.y)
    }

    fn neighbors(&self, position: GridPosition) -> [Option<GridPosition>; 4] {
        [
            (position.x > 0).then(|| GridPosition {
                x: position.x - 1,
                y: position.y,
            }),
            (position.x + 1 < self.width).then(|| GridPosition {
                x: position.x + 1,
                y: position.y,
            }),
            (position.y > 0).then(|| GridPosition {
                x: position.x,
                y: position.y - 1,
            }),
            (position.y + 1 < self.height).then(|| GridPosition {
                x: position.x,
                y: position.y + 1,
            }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_path_uses_gap_in_wall() {
        let map = ArenaMap::from_json(
            r########"{
                "schema_version": 1,
                "id": "path_test",
                "revision": "1",
                "name": "Path Test",
                "width": 7,
                "height": 7,
                "tile_size": 32,
                "tiles": [
                    "#######",
                    "#..#..#",
                    "#..#..#",
                    "#.....#",
                    "#..#..#",
                    "#..#..#",
                    "#######"
                ],
                "spawn_points": [[1, 1], [5, 1], [1, 5], [5, 5]],
                "item_spawn_points": [[2, 3]]
            }"########,
        )
        .expect("path test map");
        let start = map.tile_center(GridPosition { x: 1, y: 1 });
        let end = map.tile_center(GridPosition { x: 5, y: 1 });

        assert!(!map.has_clear_player_path(start, end));
        let path = map.find_player_path(start, end).expect("path through gap");
        assert!(
            path.iter()
                .any(|position| { map.grid_position(*position).is_some_and(|cell| cell.y == 3) })
        );
        assert!(
            path.iter()
                .all(|position| map.valid_player_position(*position))
        );
    }
}
