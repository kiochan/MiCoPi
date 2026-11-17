//! One-shot failure after an output write for scalar/grouped correction fixtures.
use crate::{application::Session, engine::graph::Renderer};
use micopi_plugin_api as abi;
use std::{cell::Cell, sync::Arc};

pub(crate) fn assert_action_event(
    session: &mut Session,
    previous: usize,
    action: &str,
    success: bool,
) {
    let journal = session.replay_journal(false).unwrap();
    assert_eq!(journal.entries.len(), previous + 1);
    let events = journal.events(previous, 1).unwrap();
    let event = &events["events"][0];
    assert_eq!(event["source"]["action"], action);
    assert_eq!(event["success"], success);
    if !success {
        assert!(event["error"].as_str().is_some_and(|s| !s.is_empty()));
        assert_eq!(event["before"], event["after"]);
        assert_eq!(event["changed_tiles"], 0);
        if let Some(changed) = event.get("changed_mask_tiles") {
            assert_eq!(changed.as_u64(), Some(0));
        }
        assert_eq!(event["visual"], false);
    }
}

pub(crate) struct FailOnceRenderer {
    inner: Arc<dyn Renderer>,
    armed: Cell<bool>,
    pub failures: Cell<usize>,
}
impl FailOnceRenderer {
    pub fn install(session: &mut Session) -> Arc<Self> {
        let renderer = Arc::new(Self {
            inner: session.doc.renderer.as_ref().unwrap().clone(),
            armed: Cell::new(false),
            failures: Cell::new(0),
        });
        session.doc.renderer = Some(renderer.clone());
        renderer
    }
    pub fn arm(&self) {
        self.armed.set(true);
    }
    fn fail(&self, output: &mut abi::Pixels) -> Result<(), String> {
        if self.armed.replace(false) {
            if output.len != 0 {
                unsafe { output.data.write(0xdeadc0de) };
            }
            self.failures.set(self.failures.get() + 1);
            return Err("Injected correction Apply failure after output write".into());
        }
        Ok(())
    }
}
impl Renderer for FailOnceRenderer {
    fn supports_masks(&self) -> bool {
        self.inner.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.inner.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.inner.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.inner.supports_transparent()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.inner.supports_blend_mode(mode)
    }
    fn render(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.fail(output)?;
        self.inner.render(nodes, layers, width, height, output)
    }
    fn render_color(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        luts: &[abi::CompositeLut],
        hsl: &[abi::HslParameters],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.fail(output)?;
        self.inner
            .render_color(nodes, layers, masks, luts, hsl, width, height, output)
    }
}
