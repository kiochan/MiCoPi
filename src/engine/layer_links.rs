//! Bounded reciprocal raster relationships, separate from selection and painting.
use super::*;

pub(super) fn validate_layer_links(metadata: &[LayerMetadata]) -> Result<(), String> {
    if metadata.len() > MAX_LAYERS {
        return Err("Layer links exceed the raster layer limit".into());
    }
    for item in metadata {
        let Some(links) = &item.links else { continue };
        if links.version != 1
            || !(2..=MAX_LAYERS).contains(&links.layer_ids.len())
            || !links.layer_ids.contains(&item.id)
            || links.layer_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || links.layer_ids.iter().any(|id| {
                *id == 0
                    || *id > u32::MAX as u64
                    || metadata
                        .iter()
                        .find(|other| other.id == *id)
                        .is_none_or(|other| other.links.as_ref() != Some(links))
            })
        {
            return Err("Invalid reciprocal layer link membership".into());
        }
    }
    Ok(())
}

/// Project validated original relationships onto retained original identities.
/// New copies never inherit membership, even if their metadata was cloned.
pub(super) fn project_layer_links(
    source: &[LayerMetadata],
    candidate: &mut [LayerMetadata],
) -> Result<(), String> {
    validate_layer_links(source)?;
    let ids: Vec<_> = candidate.iter().map(|item| item.id).collect();
    for item in candidate.iter_mut() {
        item.links = source
            .iter()
            .find(|old| old.id == item.id)
            .and_then(|old| old.links.as_ref())
            .and_then(|links| {
                let layer_ids: Vec<_> = links
                    .layer_ids
                    .iter()
                    .copied()
                    .filter(|id| ids.contains(id))
                    .collect();
                (layer_ids.len() >= 2).then_some(LayerLinks {
                    version: 1,
                    layer_ids,
                })
            });
    }
    validate_layer_links(candidate)
}

impl Document {
    pub fn layer_link_sets(&self) -> Vec<Vec<u64>> {
        let mut sets: Vec<_> = self
            .layer_metadata
            .iter()
            .filter_map(|item| {
                item.links
                    .as_ref()
                    .filter(|links| links.layer_ids.first() == Some(&item.id))
                    .map(|links| links.layer_ids.clone())
            })
            .collect();
        sets.sort();
        sets
    }

    pub fn linked_layer_ids(&self, id: u64) -> Result<Vec<u64>, String> {
        let index = self.layer_index(id).ok_or("Layer does not exist")?;
        validate_layer_links(&self.layer_metadata)?;
        Ok(self.layer_metadata[index]
            .links
            .as_ref()
            .map_or_else(|| vec![id], |links| links.layer_ids.clone()))
    }

    pub fn set_layer_links(&mut self, ids: &[u64], linked: bool) -> Result<bool, String> {
        if self.stroke.is_some() || self.mask_contact.is_some() || self.history_group.is_some() {
            return Err("Finish the current contact before editing layer links".into());
        }
        self.composition().validate_layer_metadata()?;
        if ids.is_empty()
            || ids.len() > MAX_LAYERS
            || (linked && ids.len() < 2)
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| self.layer_index(*id).is_none() || ids[..i].contains(id))
        {
            return Err(
                "Layer links require unique existing raster IDs; linking needs at least two".into(),
            );
        }
        let mut metadata = self.layer_metadata.clone();
        if linked {
            let mut members = ids.to_vec();
            for id in ids {
                if let Some(links) = &self.layer_metadata[self.layer_index(*id).unwrap()].links {
                    members.extend(&links.layer_ids);
                }
            }
            members.sort_unstable();
            members.dedup();
            for item in &mut metadata {
                if members.contains(&item.id) {
                    item.links = Some(LayerLinks {
                        version: 1,
                        layer_ids: members.clone(),
                    });
                }
            }
        } else {
            for item in &mut metadata {
                if ids.contains(&item.id) {
                    item.links = None;
                } else if let Some(links) = &mut item.links {
                    links.layer_ids.retain(|id| !ids.contains(id));
                    if links.layer_ids.len() < 2 {
                        item.links = None;
                    }
                }
            }
        }
        validate_layer_links(&metadata)?;
        if metadata == self.layer_metadata {
            return Ok(false);
        }
        let before = self.composition();
        self.layer_metadata = metadata;
        self.commit_composition(before, None);
        Ok(true)
    }
}

#[cfg(test)]
#[path = "layer_links_tests.rs"]
mod tests;
