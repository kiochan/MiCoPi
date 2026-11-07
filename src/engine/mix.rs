//! Cold capability checks for backdrop-dependent groups and explicit image mixes.
use super::super::groups::{GroupPolicy, GroupTree};
use super::*;

impl Document {
    pub fn pass_through_available(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(|r| r.supports_mix() && r.supports_transparent())
    }
    pub fn ensure_mix_capability(
        &self,
        graph: &Graph,
        groups: Option<&GroupTree>,
    ) -> Result<(), String> {
        if (graph
            .nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::Mix { .. }))
            || groups.is_some_and(|tree| {
                !tree.adjustments.is_empty()
                    || tree
                        .groups
                        .iter()
                        .any(|g| g.compositing == GroupPolicy::PassThrough)
            }))
            && self.renderer.as_ref().is_none_or(|r| !r.supports_mix())
        {
            return Err("The compositor does not support backdrop mixing".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Capabilities {
        transparent: bool,
        mix: bool,
    }
    impl super::super::super::graph::Renderer for Capabilities {
        fn supports_transparent(&self) -> bool {
            self.transparent
        }
        fn supports_mix(&self) -> bool {
            self.mix
        }
        fn render(
            &self,
            _: &[micopi_plugin_api::CompositeNode],
            _: &[micopi_plugin_api::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut micopi_plugin_api::Pixels,
        ) -> Result<(), String> {
            panic!("Availability must not invoke rendering");
        }
    }
    #[test]
    fn pass_through_availability_requires_both_group_base_and_mix_capabilities() {
        let mut doc = Document::new(2, 2);
        assert!(!doc.pass_through_available());
        for transparent in [false, true] {
            for mix in [false, true] {
                doc.renderer = Some(std::sync::Arc::new(Capabilities { transparent, mix }));
                assert_eq!(doc.pass_through_available(), transparent && mix);
                assert_eq!(
                    doc.groups_state()["pass_through_available"],
                    transparent && mix
                );
                assert_eq!(doc.renderer.as_ref().unwrap().supports_mix(), mix);
            }
        }
    }
}
