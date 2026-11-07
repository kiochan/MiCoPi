//! Typed Levels intent. Derived lookup tables are not document metadata.
use super::graph::Renderer;
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Levels {
    pub input_black: u8,
    pub input_white: u8,
    pub gamma: f64,
    pub output_black: u8,
    pub output_white: u8,
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
