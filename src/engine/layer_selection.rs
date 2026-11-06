//! Stable-ID layer selection, independent of pixel selection and pixel history.
use super::*;

impl Composition {
    pub fn validate_layer_selection(&self, active: usize) -> Result<(), String> {
        if active >= self.layers.len() {
            return Err("Invalid active layer".into());
        }
        if self.selected_layer_ids.is_empty() {
            return Ok(());
        }
        let ids: Vec<_> = if self.layer_metadata.is_empty() {
            (1..=self.layers.len() as u64).collect()
        } else {
            self.layer_metadata.iter().map(|m| m.id).collect()
        };
        let selected: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| self.selected_layer_ids.contains(id))
            .collect();
        if ids.len() != self.layers.len()
            || self.selected_layer_ids.len() < 2
            || selected != self.selected_layer_ids
            || !selected.contains(&ids[active])
        {
            return Err("Invalid selected layer identities or active membership".into());
        }
        Ok(())
    }
}

impl Document {
    pub fn layer_selected(&self, index: usize) -> bool {
        index == self.active || self.selected_layer_ids.contains(&self.layer_id(index))
    }
    pub fn selected_layer_ids(&self) -> Vec<u64> {
        (0..self.layers.len())
            .filter(|&i| self.layer_selected(i))
            .map(|i| self.layer_id(i))
            .collect()
    }
    pub fn set_layer_selection(&mut self, ids: &[u64], active: Option<u64>) -> Result<(), String> {
        if ids.is_empty()
            || ids.len() > MAX_LAYERS
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| self.layer_index(*id).is_none() || ids[..i].contains(id))
        {
            return Err("Select between one and sixteen unique existing layer IDs".into());
        }
        let current = self.layer_id(self.active);
        let explicit_active = active.is_some();
        let active = active.unwrap_or_else(|| {
            if ids.contains(&current) {
                current
            } else {
                *ids.last().unwrap()
            }
        });
        if !ids.contains(&active) {
            return Err("The active layer must belong to the layer selection".into());
        }
        self.active = self.layer_index(active).unwrap();
        if explicit_active {
            self.raster_masks.target = super::super::raster_masks::PaintTarget::LayerPixels;
        }
        self.selected_layer_ids = ids.to_vec();
        self.normalize_layer_selection();
        Ok(())
    }
    pub fn select_layer(&mut self, index: usize) {
        self.active = index;
        self.selected_layer_ids.clear();
        self.raster_masks.target = super::super::raster_masks::PaintTarget::LayerPixels;
    }
    pub(super) fn normalize_layer_selection(&mut self) {
        self.selected_layer_ids = (0..self.layers.len())
            .filter(|&i| i == self.active || self.selected_layer_ids.contains(&self.layer_id(i)))
            .map(|i| self.layer_id(i))
            .collect();
        if self.selected_layer_ids.len() == 1 {
            self.selected_layer_ids.clear();
        }
    }
    pub(crate) fn begin_history_group(&mut self) -> Result<(), String> {
        if self.history_group.is_some() {
            return Err("A layer history group is already open".into());
        }
        self.finish();
        self.history_group = Some(HistoryGroup {
            edits: Vec::new(),
            active: self.active,
            selected: self.selected_layer_ids.clone(),
        });
        Ok(())
    }
    pub(crate) fn history_group_active(&self) -> bool {
        self.history_group.is_some()
    }
    pub(crate) fn end_history_group(&mut self) -> Result<(), String> {
        self.finish();
        let group = self
            .history_group
            .take()
            .ok_or("No layer history group is open")?;
        if !group.edits.is_empty() {
            self.undo.push(Edit {
                layer: self.active,
                tiles: BTreeMap::new(),
                mask_tiles: BTreeMap::new(),
                mask_change: None,
                composition: None,
                metadata_bytes: 0,
                layer_change: None,
                group: Some(group),
            });
            self.redo.clear();
            self.prune_history();
        }
        Ok(())
    }
    /// Session's document checkpoint restores pixels/history after a failed group.
    pub(crate) fn cancel_history_group(&mut self) {
        self.history_group = None;
    }
}
