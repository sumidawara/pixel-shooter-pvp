//! サーバーで利用できる検証済みマップの一覧。

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use bevy::prelude::Resource;
use pixel_shooter_game_core::ArenaMap;
use pixel_shooter_protocol::MapSummary;

#[derive(Resource)]
pub(crate) struct MapCatalog {
    maps: BTreeMap<String, ArenaMap>,
    default_id: String,
}

impl MapCatalog {
    pub(crate) fn load_from_environment() -> Self {
        let selected_path = env::var("PIXEL_SHOOTER_MAP").ok().map(PathBuf::from);
        let directory = env::var("PIXEL_SHOOTER_MAPS_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                selected_path
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
            });

        let Some(directory) = directory else {
            return Self::from_maps(vec![ArenaMap::default()], "classic_arena")
                .expect("embedded classic arena");
        };
        let selected_id = selected_path
            .as_deref()
            .map(ArenaMap::load)
            .transpose()
            .unwrap_or_else(|error| panic!("Could not load selected map: {error}"))
            .map(|map| map.id().to_owned())
            .unwrap_or_else(|| "classic_arena".into());
        Self::load_directory(&directory, &selected_id).unwrap_or_else(|error| {
            panic!("Could not load maps from {}: {error}", directory.display())
        })
    }

    fn load_directory(directory: &Path, default_id: &str) -> Result<Self, String> {
        let entries = fs::read_dir(directory)
            .map_err(|error| format!("could not read directory: {error}"))?;
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect();
        paths.sort();
        let maps = paths
            .into_iter()
            .map(|path| {
                ArenaMap::load(&path).map_err(|error| format!("{}: {error}", path.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_maps(maps, default_id)
    }

    fn from_maps(maps: Vec<ArenaMap>, default_id: &str) -> Result<Self, String> {
        let mut by_id = BTreeMap::new();
        for map in maps {
            let id = map.id().to_owned();
            if by_id.insert(id.clone(), map).is_some() {
                return Err(format!("duplicate map id: {id}"));
            }
        }
        if by_id.is_empty() {
            return Err("no map JSON files were found".into());
        }
        if !by_id.contains_key(default_id) {
            return Err(format!("default map id was not found: {default_id}"));
        }
        Ok(Self {
            maps: by_id,
            default_id: default_id.into(),
        })
    }

    pub(crate) fn default_map(&self) -> &ArenaMap {
        self.maps
            .get(&self.default_id)
            .expect("validated default map")
    }

    pub(crate) fn get(&self, id: &str) -> Option<&ArenaMap> {
        self.maps.get(id)
    }

    pub(crate) fn summaries(&self) -> Vec<MapSummary> {
        self.maps
            .values()
            .map(|map| MapSummary {
                id: map.id().into(),
                name: map.name().into(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_maps_form_a_valid_catalog() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../maps");
        let catalog =
            MapCatalog::load_directory(&directory, "classic_arena").expect("checked-in maps");
        let summaries = catalog.summaries();

        assert_eq!(summaries.len(), 4);
        assert!(summaries.iter().any(|map| map.id == "crossroads"));
        assert!(summaries.iter().any(|map| map.id == "four_fortresses"));
        assert!(summaries.iter().any(|map| map.id == "open_range"));
    }
}
