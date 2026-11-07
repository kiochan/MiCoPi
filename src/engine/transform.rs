//! Bounded document-owned repeat recipe; independent of UI and plugin instances.
use serde::{Deserialize, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Layer,
    Selection,
    SelectionMask,
    Owners,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Nearest,
    #[default]
    Bilinear,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Stored")]
pub struct Recipe {
    pub matrix: [f64; 6],
    #[serde(
        default = "default_denominator",
        skip_serializing_if = "standard_denominator"
    )]
    pub denominator: [f64; 3],
    pub target: Target,
    pub method: Method,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owners: Option<OwnerScope>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    matrix: [f64; 6],
    #[serde(default = "default_denominator")]
    denominator: [f64; 3],
    target: Target,
    method: Method,
    #[serde(default)]
    owners: Option<OwnerScope>,
}

/// Exact captured raster identities and owned linked mask companions. Raster
/// link membership may change later; Repeat never expands this captured set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "StoredOwnerScope")]
pub struct OwnerScope {
    layers: [u64; 16],
    masks: [u64; 16],
    layer_count: u8,
    mask_count: u8,
    mask_owner_layer_id: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOwnerScope {
    version: u32,
    #[serde(deserialize_with = "bounded_owner_ids")]
    layer_ids: Vec<u64>,
    #[serde(deserialize_with = "bounded_owner_ids")]
    mask_ids: Vec<u64>,
    #[serde(default)]
    mask_owner_layer_id: Option<u64>,
}
fn bounded_owner_ids<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<u64>, D::Error> {
    struct Ids;
    impl<'de> serde::de::Visitor<'de> for Ids {
        type Value = Vec<u64>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("at most sixteen unique positive stable IDs")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u64>, A::Error> {
            let mut ids = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(16));
            while let Some(id) = seq.next_element::<u64>()? {
                if ids.len() == 16 || id == 0 || id > u32::MAX as u64 || ids.contains(&id) {
                    return Err(serde::de::Error::custom(
                        "Invalid or oversized captured owner IDs",
                    ));
                }
                ids.push(id);
            }
            Ok(ids)
        }
    }
    deserializer.deserialize_seq(Ids)
}
impl OwnerScope {
    pub fn new(
        layer_ids: &[u64],
        mask_ids: &[u64],
        mask_owner_layer_id: Option<u64>,
    ) -> Result<Self, String> {
        if layer_ids.len() > 16 || mask_ids.len() > 16 {
            return Err("Owner transform scope exceeds sixteen raster or mask IDs".into());
        }
        let mut scope = Self {
            layers: [0; 16],
            masks: [0; 16],
            layer_count: layer_ids.len() as u8,
            mask_count: mask_ids.len() as u8,
            mask_owner_layer_id,
        };
        scope.layers[..layer_ids.len()].copy_from_slice(layer_ids);
        scope.masks[..mask_ids.len()].copy_from_slice(mask_ids);
        scope.validate()?;
        Ok(scope)
    }
    pub fn layer_ids(&self) -> &[u64] {
        &self.layers[..self.layer_count as usize]
    }
    pub fn mask_ids(&self) -> &[u64] {
        &self.masks[..self.mask_count as usize]
    }
    pub fn mask_owner_layer_id(&self) -> Option<u64> {
        self.mask_owner_layer_id
    }
    pub fn is_mask_only(&self) -> bool {
        self.layer_count == 0
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.layer_count > 16 || self.mask_count > 16 {
            return Err("Invalid owner transform scope counts".into());
        }
        let valid_id = |id: u64| id != 0 && id <= u32::MAX as u64;
        let valid_ids = |ids: &[u64]| {
            ids.iter()
                .enumerate()
                .all(|(i, id)| valid_id(*id) && !ids[..i].contains(id))
        };
        if !valid_ids(self.layer_ids())
            || !valid_ids(self.mask_ids())
            || if self.is_mask_only() {
                self.mask_count != 1 || self.mask_owner_layer_id.is_none_or(|id| !valid_id(id))
            } else {
                self.mask_count > self.layer_count || self.mask_owner_layer_id.is_some()
            }
        {
            return Err("Invalid captured raster/mask transform scope".into());
        }
        Ok(())
    }
}
impl Serialize for OwnerScope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            version: u32,
            layer_ids: &'a [u64],
            mask_ids: &'a [u64],
            #[serde(skip_serializing_if = "Option::is_none")]
            mask_owner_layer_id: Option<u64>,
        }
        Wire {
            version: 1,
            layer_ids: self.layer_ids(),
            mask_ids: self.mask_ids(),
            mask_owner_layer_id: self.mask_owner_layer_id,
        }
        .serialize(serializer)
    }
}
impl TryFrom<StoredOwnerScope> for OwnerScope {
    type Error = String;
    fn try_from(stored: StoredOwnerScope) -> Result<Self, String> {
        if stored.version != 1 {
            return Err("Unsupported owner transform scope version".into());
        }
        Self::new(
            &stored.layer_ids,
            &stored.mask_ids,
            stored.mask_owner_layer_id,
        )
    }
}
impl Recipe {
    pub fn mapping(&self) -> super::homography::Mapping {
        super::homography::Mapping {
            matrix: self.matrix,
            denominator: self.denominator,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.mapping().validate()?;
        match (self.target, self.owners) {
            (Target::Owners, Some(scope)) if standard_denominator(&self.denominator) => {
                scope.validate()
            }
            (Target::Owners, _) => Err("Owner transforms require a captured affine scope".into()),
            (_, Some(_)) => Err("Legacy transform targets cannot carry an owner scope".into()),
            (_, None) => Ok(()),
        }
    }
}
impl TryFrom<Stored> for Recipe {
    type Error = String;
    fn try_from(s: Stored) -> Result<Self, String> {
        let recipe = Self {
            matrix: s.matrix,
            denominator: s.denominator,
            target: s.target,
            method: s.method,
            owners: s.owners,
        };
        recipe.validate()?;
        Ok(recipe)
    }
}

fn default_denominator() -> [f64; 3] {
    [0., 0., 1.]
}
fn standard_denominator(value: &[f64; 3]) -> bool {
    *value == default_denominator()
}
