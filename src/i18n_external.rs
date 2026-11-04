//! Poll bounded external files off the UI/input thread; publish immutable snapshots.
use super::*;
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TrySendError},
    time::{Duration, Instant, SystemTime},
};
const LIMIT: u64 = 2 * 1024 * 1024;
const INTERVAL: Duration = Duration::from_millis(500);
#[derive(Clone, PartialEq)]
enum Stamp {
    Missing,
    File(u64, Option<SystemTime>),
    Error(String),
}
struct Source {
    path: PathBuf,
    language: usize,
    stamp: Option<Stamp>,
    good: Catalog,
    error: Option<String>,
    retry_after: Option<Instant>,
}
pub(super) struct Update {
    pub(super) data: Arc<Catalogs>,
    pub(super) errors: Vec<String>,
}
pub(super) struct Reader {
    sources: Vec<Source>,
    initial: bool,
}
impl Reader {
    pub(super) fn new(directories: Vec<PathBuf>) -> Self {
        let mut seen = BTreeSet::new();
        let sources = directories
            .into_iter()
            .filter(|d| seen.insert(d.clone()))
            .flat_map(|d| {
                Language::ALL
                    .into_iter()
                    .enumerate()
                    .skip(1)
                    .map(move |(language, l)| Source {
                        path: d.join(format!("{}.json", l.code())),
                        language,
                        stamp: None,
                        good: Catalog::new(),
                        error: None,
                        retry_after: None,
                    })
            })
            .collect();
        Self {
            sources,
            initial: true,
        }
    }
    pub(super) fn refresh(&mut self) -> Option<Update> {
        let mut changed = std::mem::take(&mut self.initial);
        for source in &mut self.sources {
            let stamp = match fs::metadata(&source.path) {
                Ok(m) => Stamp::File(m.len(), m.modified().ok()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stamp::Missing,
                Err(e) => Stamp::Error(e.to_string()),
            };
            if source.stamp.as_ref() == Some(&stamp)
                && source.retry_after.is_none_or(|t| Instant::now() < t)
            {
                continue;
            }
            source.stamp = Some(stamp.clone());
            let result = match stamp {
                Stamp::Missing => Ok(Catalog::new()),
                Stamp::Error(e) => Err(e),
                Stamp::File(size, _) if size > LIMIT => Err("Language file exceeds 2 MiB".into()),
                Stamp::File(_, _) => read(&source.path),
            };
            match result {
                Ok(catalog) => {
                    changed |= source.good != catalog || source.error.is_some();
                    source.good = catalog;
                    source.error = None;
                    source.retry_after = None;
                }
                Err(e) => {
                    changed |= source.error.as_ref() != Some(&e);
                    source.error = Some(e);
                    source.retry_after = Some(Instant::now() + Duration::from_secs(2));
                }
            }
        }
        if !changed {
            return None;
        }
        let mut values = std::array::from_fn(|i| {
            if i == 0 {
                english().clone()
            } else {
                Catalog::new()
            }
        });
        let mut errors = vec![];
        for source in &self.sources {
            values[source.language].extend(source.good.clone());
            if let Some(error) = &source.error {
                errors.push(format!("{}: {error}", source.path.display()));
            }
        }
        Some(Update {
            data: Arc::new(build_catalogs(values)),
            errors,
        })
    }
}
fn read(path: &std::path::Path) -> Result<Catalog, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Language file exceeds 2 MiB".into());
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let catalog: Catalog = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let slots = |s: &str| -> Option<Vec<String>> {
        let mut result: Vec<_> = parts(s)?
            .into_iter()
            .filter(|(_, slot)| *slot)
            .map(|(s, _)| s.to_owned())
            .collect();
        result.sort();
        Some(result)
    };
    for (key, value) in &catalog {
        if key.trim().is_empty() || value.trim().is_empty() {
            return Err("Language keys and values must be nonempty strings".into());
        }
        if slots(key).is_none() || slots(key) != slots(value) {
            return Err(format!("Format placeholders differ for {key:?}"));
        }
    }
    Ok(catalog)
}
/// Application language files first, per-profile overrides second. Only English is embedded.
pub struct HotReload {
    stop: Sender<()>,
    updates: Receiver<Update>,
}
impl HotReload {
    pub fn new(directories: Vec<PathBuf>) -> Self {
        let mut reader = Reader::new(directories);
        let first = reader.refresh().expect("initial catalog snapshot");
        install(first.data.clone());
        let (tx, updates) = mpsc::sync_channel(1);
        let (stop, shutdown) = mpsc::channel();
        std::thread::Builder::new()
            .name("micopi-locales".into())
            .spawn(move || {
                let mut pending = Some(first);
                loop {
                    if let Some(update) = pending.take() {
                        match tx.try_send(update) {
                            Ok(()) => {}
                            Err(TrySendError::Full(update)) => pending = Some(update),
                            Err(TrySendError::Disconnected(_)) => return,
                        }
                    }
                    match shutdown.recv_timeout(INTERVAL) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        _ => return,
                    }
                    if let Some(update) = reader.refresh() {
                        pending = Some(update);
                    }
                }
            })
            .expect("language watcher thread");
        Self { stop, updates }
    }
    /// Called at a UI frame boundary. The receiver queue is bounded to one snapshot.
    pub fn poll(&self) -> Option<Vec<String>> {
        let update = self.updates.try_iter().last()?;
        install(update.data);
        Some(update.errors)
    }
}
impl Drop for HotReload {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-output/locales")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn apply(reader: &mut Reader) -> Vec<String> {
        let update = reader.refresh().expect("catalog or diagnostic changed");
        install(update.data);
        update.errors
    }
    #[test]
    fn only_english_is_available_without_external_files() {
        let dir = root();
        fs::write(dir.join("en.json"), r#"{"File":"Changed English"}"#).unwrap();
        let mut reader = Reader::new(vec![dir]);
        assert!(apply(&mut reader).is_empty());
        for language in Language::ALL {
            assert_eq!(text_in(language, "File"), "File");
        }
        assert!(reader.refresh().is_none());
    }
    #[test]
    fn partial_bom_catalogs_reload_atomically_and_keep_owned_text_valid() {
        let dir = root();
        let file = dir.join("ja.json");
        fs::write(&file, "\u{feff}{\"File\":\"File A\",\"Layer {:02}\":\"Row {:02}\",\"Plugin {name}\":\"Addon {name}\"}").unwrap();
        let mut reader = Reader::new(vec![dir]);
        assert!(apply(&mut reader).is_empty());
        let old = text_in(Language::Japanese, "File");
        assert_eq!(old, "File A");
        assert_eq!(text_in(Language::Japanese, "Edit"), "Edit");
        assert_eq!(translated_in(Language::Japanese, "Layer 03"), "Row 03");
        assert_eq!(
            translated_in(Language::Japanese, "Plugin example"),
            "Addon example"
        );
        fs::write(&file, r#"{"File":"File BBB"}"#).unwrap();
        assert!(apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Japanese, "File"), "File BBB");
        assert_eq!(old, "File A");
        fs::write(&file, r#"{"File":"Broken","Layer {:02}":"Wrong {}"}"#).unwrap();
        assert!(!apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Japanese, "File"), "File BBB");
        assert!(reader.refresh().is_none());
        fs::write(&file, "{").unwrap();
        assert!(!apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Japanese, "File"), "File BBB");
        fs::write(&file, vec![b' '; LIMIT as usize + 1]).unwrap();
        assert!(!apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Japanese, "File"), "File BBB");
        fs::remove_file(file).unwrap();
        assert!(apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Japanese, "File"), "File");
    }
    #[test]
    fn profile_overrides_merge_over_application_files_and_removal_restores_fallback() {
        let app = root();
        let profile = root();
        fs::write(
            app.join("zh-CN.json"),
            r#"{"File":"App File","Edit":"App Edit"}"#,
        )
        .unwrap();
        fs::write(profile.join("zh-CN.json"), r#"{"File":"Profile File"}"#).unwrap();
        let mut reader = Reader::new(vec![app.clone(), profile.clone()]);
        assert!(apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Chinese, "File"), "Profile File");
        assert_eq!(text_in(Language::Chinese, "Edit"), "App Edit");
        fs::remove_file(profile.join("zh-CN.json")).unwrap();
        assert!(apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Chinese, "File"), "App File");
        fs::remove_file(app.join("zh-CN.json")).unwrap();
        assert!(apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Chinese, "File"), "File");
    }
    #[test]
    fn worker_publishes_edits_without_changing_active_language() {
        let dir = root();
        let watcher = HotReload::new(vec![dir.clone()]);
        set_language(Language::Korean);
        fs::write(dir.join("ko.json"), r#"{"File":"Live File"}"#).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while text("File") != "Live File" {
            watcher.poll();
            assert!(
                std::time::Instant::now() < deadline,
                "watcher did not publish the edit"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(current(), Language::Korean);
        assert!(search_text("File").contains("Live File"));
    }
    #[test]
    fn failed_files_retry_even_if_an_editor_preserves_file_metadata() {
        let dir = root();
        let file = dir.join("ja.json");
        let valid = r#"{"File":"Repaired"}"#;
        fs::write(&file, format!("{{{}", " ".repeat(valid.len() - 1))).unwrap();
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        let mut reader = Reader::new(vec![dir]);
        assert!(!apply(&mut reader).is_empty());
        fs::write(&file, valid).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert!(reader.refresh().is_none());
        reader
            .sources
            .iter_mut()
            .find(|s| s.language == 2)
            .unwrap()
            .retry_after = Some(Instant::now());
        assert!(apply(&mut reader).is_empty());
        assert_eq!(text_in(Language::Japanese, "File"), "Repaired");
    }
}
