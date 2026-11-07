//! Typed Levels intent. Derived lookup tables are not document metadata.
use super::{curves::ColorIntent, graph::Renderer};
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Cold, bounded rendering cache. A failed rebuild preserves the previous plan.
/// Holding the renderer also holds the DLL while its callbacks may be used.
#[derive(Clone)]
pub struct PreparedLevelsPlan {
    provider: Arc<dyn Renderer>,
    intents: Vec<ColorIntent>,
    tables: Vec<abi::CompositeLut>,
}
impl PreparedLevelsPlan {
    pub fn matches(&self, provider: &Arc<dyn Renderer>, intents: &[Levels]) -> bool {
        Arc::ptr_eq(&self.provider, provider)
            && self.intents.len() == intents.len()
            && self
                .intents
                .iter()
                .zip(intents)
                .all(|(a, b)| matches!(a,ColorIntent::Levels(p) if p==b))
    }
    pub fn new(provider: Arc<dyn Renderer>, intents: &[Levels]) -> Result<Self, String> {
        Self::new_color(
            provider,
            &intents
                .iter()
                .copied()
                .map(ColorIntent::Levels)
                .collect::<Vec<_>>(),
        )
    }
    pub fn rebuild(
        &mut self,
        provider: Arc<dyn Renderer>,
        intents: &[Levels],
    ) -> Result<(), String> {
        if self.matches(&provider, intents) {
            return Ok(());
        }
        self.rebuild_color(
            provider,
            &intents
                .iter()
                .copied()
                .map(ColorIntent::Levels)
                .collect::<Vec<_>>(),
        )
    }
    pub fn matches_color(&self, provider: &Arc<dyn Renderer>, intents: &[ColorIntent]) -> bool {
        Arc::ptr_eq(&self.provider, provider) && self.intents == intents
    }
    fn validate_color(intents: &[ColorIntent]) -> Result<usize, String> {
        let count = intents.iter().map(ColorIntent::table_count).sum::<usize>();
        if count > 64 {
            return Err("Color table limit exceeded".into());
        }
        for intent in intents {
            intent.validate()?;
        }
        Ok(count)
    }
    pub fn new_color(provider: Arc<dyn Renderer>, intents: &[ColorIntent]) -> Result<Self, String> {
        let count = Self::validate_color(intents)?;
        let mut tables = Vec::with_capacity(count);
        for intent in intents {
            intent.prepare(provider.as_ref(), &mut tables)?;
        }
        Ok(Self {
            provider,
            intents: intents.to_vec(),
            tables,
        })
    }
    /// Stable ordered intents reuse all storage; changed plans publish only
    /// after every preparation succeeds. Each unchanged slot keeps its tables.
    pub fn rebuild_color(
        &mut self,
        provider: Arc<dyn Renderer>,
        intents: &[ColorIntent],
    ) -> Result<(), String> {
        let count = Self::validate_color(intents)?;
        if self.matches_color(&provider, intents) {
            return Ok(());
        }
        let same_provider = Arc::ptr_eq(&self.provider, &provider);
        let mut tables = Vec::with_capacity(count);
        let mut old_start = 0;
        for (slot, intent) in intents.iter().enumerate() {
            if same_provider && self.intents.get(slot) == Some(intent) {
                tables.extend_from_slice(&self.tables[old_start..old_start + intent.table_count()]);
            } else {
                intent.prepare(provider.as_ref(), &mut tables)?;
            }
            old_start += self.intents.get(slot).map_or(0, ColorIntent::table_count);
        }
        self.provider = provider;
        self.intents = intents.to_vec();
        self.tables = tables;
        Ok(())
    }
    pub fn tables(&self) -> &[abi::CompositeLut] {
        &self.tables
    }
    pub fn render(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.provider
            .render_prepared(nodes, layers, masks, &self.tables, width, height, output)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Levels {
    #[serde(deserialize_with = "byte")]
    pub input_black: u8,
    #[serde(deserialize_with = "byte")]
    pub input_white: u8,
    pub gamma: f64,
    #[serde(deserialize_with = "byte")]
    pub output_black: u8,
    #[serde(deserialize_with = "byte")]
    pub output_white: u8,
}
pub(super) fn byte<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let number = value
        .as_f64()
        .filter(|n| n.is_finite() && (0. ..=255.).contains(n) && n.fract() == 0.)
        .ok_or_else(|| {
            <D::Error as serde::de::Error>::custom(
                "Levels endpoints require integral bytes from 0 to 255",
            )
        })?;
    Ok(number as u8)
}
impl Default for Levels {
    fn default() -> Self {
        Self {
            input_black: 0,
            input_white: 255,
            gamma: 1.,
            output_black: 0,
            output_white: 255,
        }
    }
}
impl Levels {
    pub fn schema() -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{
            "input_black":{"type":"integer","minimum":0,"maximum":255},
            "input_white":{"type":"integer","minimum":0,"maximum":255},
            "gamma":{"type":"number","minimum":0.1,"maximum":10},
            "output_black":{"type":"integer","minimum":0,"maximum":255},
            "output_white":{"type":"integer","minimum":0,"maximum":255}},
            "required":["input_black","input_white","gamma","output_black","output_white"],"additionalProperties":false})
    }
    pub fn validate(self) -> Result<(), String> {
        if self.input_black >= self.input_white
            || !self.gamma.is_finite()
            || !(0.1..=10.).contains(&self.gamma)
        {
            return Err(
                "Levels require input black < white and finite gamma from 0.1 to 10".into(),
            );
        }
        Ok(())
    }
    pub fn parameters(self) -> abi::LevelsParameters {
        abi::LevelsParameters {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::LevelsParameters>() as u32,
            input_black: u32::from(self.input_black),
            input_white: u32::from(self.input_white),
            output_black: u32::from(self.output_black),
            output_white: u32::from(self.output_white),
            gamma: self.gamma,
        }
    }
    /// Cold plan preparation. Pixel rendering only borrows this fixed byte table.
    pub fn prepare(self, renderer: &dyn Renderer) -> Result<abi::CompositeLut, String> {
        self.validate()?;
        if !renderer.supports_levels() {
            return Err("The compositor does not support Levels rendering".into());
        }
        renderer.prepare_levels(&self.parameters())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[derive(Default)]
    struct Provider {
        calls: Cell<usize>,
        fail_at: Cell<Option<usize>>,
    }
    impl Renderer for Provider {
        fn supports_curves(&self) -> bool {
            true
        }
        fn prepare_curves(&self, _: &abi::CurvesParameters) -> Result<abi::CompositeLut, String> {
            self.prepare_levels(&Levels::default().parameters())
        }
        fn supports_levels(&self) -> bool {
            true
        }
        fn prepare_levels(&self, _: &abi::LevelsParameters) -> Result<abi::CompositeLut, String> {
            let call = self.calls.get() + 1;
            self.calls.set(call);
            if self.fail_at.get() == Some(call) {
                return Err("injected prepare failure".into());
            }
            Ok(abi::CompositeLut {
                values: [call as u8; 256],
            })
        }
        fn render(
            &self,
            _: &[abi::CompositeNode],
            _: &[abi::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut abi::Pixels,
        ) -> Result<(), String> {
            Err("unexpected legacy render".into())
        }
    }
    #[test]
    fn mixed_color_cache_reuses_variable_width_slots_and_rolls_back_failed_rebuilds() {
        use crate::engine::curves::Curves;
        let provider = Arc::new(Provider::default());
        let mut intents = vec![
            ColorIntent::Curves(Curves::default()),
            ColorIntent::Levels(Levels::default()),
        ];
        let mut plan = PreparedLevelsPlan::new_color(provider.clone(), &intents).unwrap();
        assert_eq!(provider.calls.get(), 5);
        assert_eq!(plan.tables().len(), 4);
        let pointer = plan.tables().as_ptr();
        for _ in 0..8 {
            plan.rebuild_color(provider.clone(), &intents).unwrap();
        }
        assert_eq!(provider.calls.get(), 5);
        assert_eq!(plan.tables().as_ptr(), pointer);
        intents[1] = ColorIntent::Levels(Levels {
            gamma: 2.,
            ..Default::default()
        });
        plan.rebuild_color(provider.clone(), &intents).unwrap();
        assert_eq!(provider.calls.get(), 6);
        assert_eq!(plan.tables()[3].values, [6; 256]);
        let pointer = plan.tables().as_ptr();
        if let ColorIntent::Curves(p) = &mut intents[0] {
            p.red.points[1].output = 99;
        }
        provider.fail_at.set(Some(8));
        assert!(plan.rebuild_color(provider.clone(), &intents).is_err());
        assert_eq!(plan.tables().as_ptr(), pointer);
        assert_eq!(plan.tables()[3].values, [6; 256]);
        let calls = provider.calls.get();
        assert!(PreparedLevelsPlan::new_color(
            provider.clone(),
            &vec![ColorIntent::Curves(Curves::default()); 22]
        )
        .is_err());
        assert_eq!(provider.calls.get(), calls);
        // Changing a three-table slot into one must copy the following slot
        // from its old offset, rather than its new output-table offset.
        let mut changed = vec![
            ColorIntent::Levels(Levels::default()),
            ColorIntent::Levels(Levels {
                gamma: 2.,
                ..Default::default()
            }),
        ];
        provider.fail_at.set(None);
        plan.rebuild_color(provider.clone(), &changed).unwrap();
        assert_eq!(plan.tables().len(), 2);
        assert_eq!(plan.tables()[1].values, [6; 256]);
        changed.clear();
        plan.rebuild_color(provider, &changed).unwrap();
        assert!(plan.tables().is_empty());
    }
    #[test]
    fn cache_reuses_unchanged_slots_and_publishes_only_complete_rebuilds() {
        let provider = Arc::new(Provider::default());
        let mut intents = [
            Levels::default(),
            Levels {
                gamma: 2.,
                ..Default::default()
            },
        ];
        let mut plan = PreparedLevelsPlan::new(provider.clone(), &intents).unwrap();
        assert_eq!(provider.calls.get(), 2);
        let pointer = plan.tables().as_ptr();
        for _ in 0..32 {
            plan.rebuild(provider.clone(), &intents).unwrap();
        }
        assert_eq!(provider.calls.get(), 2);
        assert_eq!(plan.tables().as_ptr(), pointer);
        intents[1].gamma = 3.;
        plan.rebuild(provider.clone(), &intents).unwrap();
        assert_eq!(provider.calls.get(), 3);
        assert_eq!(plan.tables()[0].values, [1; 256]);
        assert_eq!(plan.tables()[1].values, [3; 256]);
        let pointer = plan.tables().as_ptr();
        intents[0].gamma = 4.;
        intents[1].gamma = 5.;
        provider.fail_at.set(Some(5));
        assert!(plan.rebuild(provider.clone(), &intents).is_err());
        assert_eq!(provider.calls.get(), 5);
        assert_eq!(plan.tables().as_ptr(), pointer);
        assert_eq!(plan.tables()[0].values, [1; 256]);
        assert_eq!(plan.tables()[1].values, [3; 256]);
        provider.fail_at.set(None);
        let replacement = Arc::new(Provider::default());
        plan.rebuild(replacement.clone(), &intents).unwrap();
        assert_eq!(replacement.calls.get(), 2);
        assert_eq!(plan.tables()[1].values, [2; 256]);
    }
    #[test]
    fn cache_preflights_every_intent_and_table_limit_before_native_preparation() {
        let provider = Arc::new(Provider::default());
        assert!(PreparedLevelsPlan::new(provider.clone(), &[Levels::default(); 65]).is_err());
        assert_eq!(provider.calls.get(), 0);
        let mut plan = PreparedLevelsPlan::new(provider.clone(), &[Levels::default()]).unwrap();
        let bad = [
            Levels::default(),
            Levels {
                gamma: f64::NAN,
                ..Default::default()
            },
        ];
        assert!(plan.rebuild(provider.clone(), &bad).is_err());
        assert_eq!(provider.calls.get(), 1);
        assert_eq!(plan.tables().len(), 1);
        assert_eq!(plan.tables()[0].values, [1; 256]);
    }
    #[test]
    fn prepared_levels_table_is_reused_by_shared_renderer_with_exact_alpha() {
        let registry = crate::plugins::Registry::load(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let renderer = registry.native_plugins["compositing"].clone();
        let table = Levels {
            gamma: 2.,
            ..Default::default()
        }
        .prepare(renderer.as_ref())
        .unwrap();
        let tables = [table];
        let nodes = [
            abi::CompositeNode {
                kind: 0,
                a: 0,
                b: 0,
                mode: 0,
                amount: 1.,
            },
            abi::CompositeNode {
                kind: 9,
                a: 0,
                b: 0,
                mode: 0,
                amount: 1.,
            },
            abi::CompositeNode {
                kind: 3,
                a: 1,
                b: 0,
                mode: 0,
                amount: 1.,
            },
        ];
        for (source, expected) in [
            (0x80404040, 0x80808080),
            (0x7fffffff, 0x7fffffff),
            (0x01000000, 0x01000000),
        ] {
            let pixels = [source];
            let layers = [abi::CompositeLayer {
                data: pixels.as_ptr(),
                len: 1,
                opacity: 255,
                visible: 1,
                mode: 0,
            }];
            let mut output = [0];
            let mut target = abi::Pixels {
                data: output.as_mut_ptr(),
                len: 1,
                width: 1,
                height: 1,
                stride: 1,
                origin_x: 0,
                origin_y: 0,
            };
            renderer
                .render_prepared(&nodes, &layers, &[], &tables, 1, 1, &mut target)
                .unwrap();
            assert_eq!(output, [expected]);
            assert_eq!(pixels, [source]);
            assert_eq!(tables[0].values[64], 128);
        }
    }
    #[test]
    fn old_renderer_defaults_reject_levels_before_rendering() {
        struct Legacy;
        impl Renderer for Legacy {
            fn render(
                &self,
                _: &[abi::CompositeNode],
                _: &[abi::CompositeLayer],
                _: u32,
                _: u32,
                _: &mut abi::Pixels,
            ) -> Result<(), String> {
                panic!("Unsupported Levels must not reach legacy rendering")
            }
        }
        assert!(Levels::default().prepare(&Legacy).is_err());
        let mut target = abi::Pixels {
            data: std::ptr::null_mut(),
            len: 0,
            width: 1,
            height: 1,
            stride: 1,
            origin_x: 0,
            origin_y: 0,
        };
        assert!(Legacy
            .render_prepared(
                &[],
                &[],
                &[],
                &[abi::CompositeLut { values: [0; 256] }],
                1,
                1,
                &mut target
            )
            .is_err());
    }
    #[test]
    fn typed_levels_reject_unknown_or_out_of_byte_fields_and_validate_gamma() {
        for raw in [
            r#"{"input_black":-1}"#,
            r#"{"input_white":256}"#,
            r#"{"input_black":1.5}"#,
            r#"{"lut":[]}"#,
        ] {
            assert!(serde_json::from_str::<Levels>(raw).is_err());
        }
        for gamma in [f64::NAN, f64::INFINITY, 0.099999999, 10.000000001] {
            assert!(Levels {
                gamma,
                ..Default::default()
            }
            .validate()
            .is_err());
        }
        assert!(Levels {
            input_black: 255,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Levels {
            output_black: 255,
            output_white: 0,
            ..Default::default()
        }
        .validate()
        .is_ok());
        let value = serde_json::to_value(Levels::default()).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        assert_eq!(value["gamma"], 1.);
        assert_eq!(
            serde_json::from_value::<Levels>(value).unwrap(),
            Levels::default()
        );
    }
}
