//! Built-in system plugin action handlers; callable without a GUI.
use crate::application::{ActionError, ActionResult, Operation, Session};
use serde_json::{json, Value};
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    match op {
        State => Ok(s.state()),
        Plugins => Ok(s.registry.describe()),
        Events => {
            let after = args["after"].as_u64().unwrap_or(0);
            Ok(
                json!({"events":s.events.iter().filter(|e|e.sequence>after).collect::<Vec<_>>(),"latest":s.sequence,"oldest":s.events.front().map(|e|e.sequence)}),
            )
        }

        _ => Err(ActionError::new(
            "internal",
            "operation routed to wrong plugin",
        )),
    }
}
