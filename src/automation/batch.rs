use crate::application::{Action, ActionError, Session};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;
pub fn failed(result: &Value) -> bool {
    result["result"]["ok"] == false
}
pub fn line(reader: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut bytes = vec![];
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            };
        }
        let length = buf
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n + 1)
            .unwrap_or(buf.len());
        if bytes.len() + length > MAX_MESSAGE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "message exceeds 8 MB",
            ));
        }
        let done = buf[length - 1] == b'\n';
        bytes.extend_from_slice(&buf[..length]);
        reader.consume(length);
        if done {
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e));
        }
    }
}
pub fn run(
    session: &mut Session,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<(), ActionError> {
    let mut failure = false;
    while let Some(text) = line(reader)? {
        if text.trim().is_empty() {
            continue;
        }
        let result = serde_json::from_str::<Action>(&text)
            .map_err(ActionError::from)
            .and_then(|a| session.apply(a));
        let value = match result {
            Ok(result) => {
                failure = failed(&result);
                json!({"ok":!failure,"data":result})
            }
            Err(error) => {
                failure = true;
                json!({"ok":false,"error":error,"revision":session.revision})
            }
        };
        serde_json::to_writer(&mut *writer, &value)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        if failure {
            break;
        }
    }
    session.finish_stroke();
    if failure {
        Err(ActionError::new(
            "batch_failed",
            "Script stopped at the first error",
        ))
    } else {
        Ok(())
    }
}
pub trait Backend {
    fn actions(&mut self) -> Result<Value, ActionError>;
    fn dispatch(&mut self, action: Action) -> Result<Value, ActionError>;
}
impl Backend for Session {
    fn actions(&mut self) -> Result<Value, ActionError> {
        Ok(json!(self.registry.actions.values().map(|a|json!({"id":a.id,"title":a.title,"inputSchema":a.schema(),"outputSchema":a.output_schema,"readOnly":a.is_read_only(),"owner":self.registry.owners.get(&a.id)})).collect::<Vec<_>>()))
    }
    fn dispatch(&mut self, action: Action) -> Result<Value, ActionError> {
        self.apply(action)
    }
}
