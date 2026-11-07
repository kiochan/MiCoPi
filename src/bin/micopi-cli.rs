use micopi::{
    application::{Action, Session},
    automation::{
        batch::{self, Backend},
        mcp::Server,
    },
    plugins::Registry,
};
use std::{
    io::{self, BufReader},
    path::PathBuf,
};
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let option = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
    };
    if args.is_empty() || args.iter().any(|a| a == "--help") {
        println!("MiCoPi CLI\n  actions [--plugins DIR]\n  run FILE.jsonl [--plugins DIR]\n  mcp [--plugins DIR] [--attach SESSION.json]\n  call ACTION ARGS_JSON [--attach SESSION.json]\nWithout --attach the runtime is completely headless.");
        return Ok(());
    }
    #[cfg(windows)]
    if let Some(path) = option("--attach") {
        let mut remote = micopi::automation::ipc::Remote::from_file(&path)?;
        match args[0].as_str() {
            "mcp" => {
                return Ok(
                    Server::new(remote).serve(&mut io::stdin().lock(), &mut io::stdout().lock())?
                )
            }
            "actions" => println!("{}", remote.actions()?),
            "call" => {
                let action = args.get(1).ok_or("Missing action ID")?;
                let value = serde_json::from_str(args.get(2).map(String::as_str).unwrap_or("{}"))?;
                let result = remote.dispatch(Action::new(action, value))?;
                println!("{result}");
                if batch::failed(&result) {
                    return Err(
                        "pipeline_failed: inspect the structured result for completed steps".into(),
                    );
                }
            }
            _ => return Err("Attach mode supports mcp/actions/call".into()),
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    if option("--attach").is_some() {
        return Err("GUI attachment currently requires Windows".into());
    }
    let root = option("--plugins").unwrap_or_else(|| {
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("plugins")
    });
    let config_path = option("--config-dir")
        .map(|p| p.join("settings.json"))
        .unwrap_or_else(micopi::ui::keymap::Keymap::path);
    let settings =
        micopi::configuration::Settings::load(config_path).map_err(std::io::Error::other)?;
    let registry = Registry::load_with_disabled(&root, &settings.plugins.disabled)?;
    for d in &registry.diagnostics {
        eprintln!("plugin {}: {}", d.path.display(), d.error);
    }
    let mut session = Session::new(registry)?;
    session.settings_path = Some(settings.path.clone());
    session.load_profile_presets(&settings)?;
    session.apply_selection_defaults(settings.selection);
    session.apply_movement_defaults(settings.movement);
    if settings.has_painting_preferences() {
        settings.painting.apply(&mut session.parameters);
    }
    match args[0].as_str() {
        "mcp" => Server::new(session).serve(&mut io::stdin().lock(), &mut io::stdout().lock())?,
        "actions" => println!("{}", serde_json::to_string_pretty(&session.actions()?)?),
        "run" => {
            let path = args.get(1).ok_or("Missing JSONL script")?;
            batch::run(
                &mut session,
                &mut BufReader::new(std::fs::File::open(path)?),
                &mut io::stdout().lock(),
            )?;
        }
        "call" => {
            let id = args.get(1).ok_or("Missing action ID")?;
            let value = serde_json::from_str(args.get(2).map(String::as_str).unwrap_or("{}"))?;
            let result = session.apply(Action::new(id, value))?;
            println!("{result}");
            if batch::failed(&result) {
                return Err(
                    "pipeline_failed: inspect the structured result for completed steps".into(),
                );
            }
        }
        _ => return Err("Unknown command. Use --help".into()),
    }
    Ok(())
}
