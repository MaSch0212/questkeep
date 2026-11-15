use chrono::Utc;

use super::*;
use crate::db::NewGameRecord;
use crate::model::{GameQuery, ScriptOptions, ScriptSpec, TriState};

struct Setup {
    _tmp: tempfile::TempDir,
    db: Arc<Database>,
    scripts: Scripts,
    notifications: Notifications,
    events: EventBus,
    game: Uuid,
}

fn setup() -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(tmp.path());
    paths.ensure_dirs().unwrap();
    let db = Arc::new(Database::open_in_memory().unwrap());
    let events = EventBus::new();
    let notifications = Notifications::in_memory(&events);
    let game = db
        .insert_game(
            &NewGameRecord {
                name: "Portal 2".into(),
                source: "steam".into(),
                install_dir: Some(tmp.path().display().to_string()),
                ..Default::default()
            },
            Utc::now(),
        )
        .unwrap()
        .id;
    let scripts = Scripts::new(
        db.clone(),
        paths.clone(),
        ImageCache::new(paths),
        ProviderRegistry::new(),
        notifications.clone(),
        events.clone(),
    );
    Setup {
        _tmp: tmp,
        db,
        scripts,
        notifications,
        events,
        game,
    }
}

fn input(name: &str, kind: ScriptKind, game: Option<Uuid>) -> ScriptInput {
    ScriptInput {
        id: None,
        name: name.into(),
        kind,
        enabled: true,
        game_id: game,
        filter: None,
        spec: ScriptSpec::default(),
    }
}

fn names(scripts: &[Script]) -> Vec<&str> {
    scripts.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn resolves_the_run_order_with_the_games_own_scripts() {
    let s = setup();
    let save = |i: ScriptInput| s.scripts.save(&i).unwrap();
    let a = save(input("A", ScriptKind::Pre, None));
    let b = save(input("B", ScriptKind::Pre, None));
    save(input("Own", ScriptKind::Pre, Some(s.game)));
    let mut favorites = input("Favorites only", ScriptKind::Pre, None);
    favorites.filter = Some(GameQuery {
        favorite: TriState::Yes,
        ..GameQuery::default()
    });
    save(favorites);
    let mut off = input("Off", ScriptKind::Pre, None);
    off.enabled = false;
    save(off);
    save(input("Post", ScriptKind::Post, None));
    save(input("Own post", ScriptKind::Post, Some(s.game)));
    save(input("Global action", ScriptKind::Action, None));
    save(input("Own action", ScriptKind::Action, Some(s.game)));

    // By default own pre scripts run last, own post scripts first, own actions first.
    let pre = s.scripts.resolve(s.game, ScriptKind::Pre).unwrap();
    assert_eq!(names(&pre), ["A", "B", "Own"]);
    let post = s.scripts.resolve(s.game, ScriptKind::Post).unwrap();
    assert_eq!(names(&post), ["Own post", "Post"]);
    let actions = s.scripts.resolve(s.game, ScriptKind::Action).unwrap();
    assert_eq!(names(&actions), ["Own action", "Global action"]);

    s.scripts
        .set_global_order(
            ScriptKind::Pre,
            &[
                ScriptSlot::Script { id: b.id },
                ScriptSlot::GameScripts,
                ScriptSlot::Script { id: a.id },
            ],
        )
        .unwrap();
    let pre = s.scripts.resolve(s.game, ScriptKind::Pre).unwrap();
    assert_eq!(names(&pre), ["B", "Own", "A"]);

    s.scripts.set_run_global(s.game, false).unwrap();
    let pre = s.scripts.resolve(s.game, ScriptKind::Pre).unwrap();
    assert_eq!(names(&pre), ["Own"]);
    let game = s.scripts.for_game(s.game).unwrap();
    assert!(!game.run_global);
    assert_eq!(game.own.len(), 3);
}

#[test]
fn keeps_inline_scripts_in_files() {
    let s = setup();
    let mut i = input("Inline", ScriptKind::Action, None);
    i.spec.source = ScriptSource::Inline;
    assert!(s.scripts.save(&i).is_err(), "a runtime is needed");
    i.spec.runtime = Some(RuntimeId::Python);
    i.spec.content = Some("print('hi')".into());
    let saved = s.scripts.save(&i).unwrap();
    let file = s.scripts.script_file(&saved).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "print('hi')");
    assert_eq!(saved.spec.content.as_deref(), Some("print('hi')"));

    // Another runtime: another extension, the old file goes.
    i.id = Some(saved.id);
    i.spec.runtime = Some(RuntimeId::Bash);
    let saved = s.scripts.save(&i).unwrap();
    assert!(!file.exists());
    let file = s.scripts.script_file(&saved).unwrap();
    assert!(file.exists());

    s.db.delete_script(saved.id).unwrap();
    s.scripts.clean_up_files();
    assert!(!file.exists());
}

fn shell(command: &str) -> ScriptSpec {
    let (program, args) = if cfg!(windows) {
        ("cmd.exe", format!("/d /c \"{command}\""))
    } else {
        ("sh", format!("-c '{command}'"))
    };
    ScriptSpec {
        program: program.into(),
        args,
        options: ScriptOptions {
            wait: true,
            timeout_seconds: 20,
            ..ScriptOptions::default()
        },
        ..ScriptSpec::default()
    }
}

#[test]
fn runs_programs_with_the_games_variables() {
    let s = setup();
    let out = s._tmp.path().join("out.txt");
    let command = if cfg!(windows) {
        format!("echo %QK_GAME_NAME%^|%QK_PHASE%> {}", out.display())
    } else {
        format!(
            "printf \"%s|%s\" \"$QK_GAME_NAME\" \"$QK_PHASE\" > {}",
            out.display()
        )
    };
    let mut i = input("Write", ScriptKind::Pre, None);
    i.spec = shell(&command);
    let script = s.scripts.save(&i).unwrap();
    s.scripts
        .run(&script, s.game, None, ScriptKind::Pre, None)
        .unwrap();
    let written = std::fs::read_to_string(&out).unwrap();
    assert_eq!(written.trim(), "Portal 2|pre");
    assert!(s.notifications.list().is_empty());
}

#[test]
fn reports_failures_with_their_log() {
    let s = setup();
    let mut i = input("Fails", ScriptKind::Post, None);
    i.spec = shell("echo broken & exit 3");
    let script = s.scripts.save(&i).unwrap();
    let err = s
        .scripts
        .run(&script, s.game, None, ScriptKind::Post, None)
        .unwrap_err();
    assert!(err.contains("exit code 3"), "{err}");
    let note = &s.notifications.list()[0];
    let Some(NotificationAction::OpenFile { path }) = &note.action else {
        panic!("{note:?}");
    };
    assert!(std::fs::read_to_string(path).unwrap().contains("broken"));

    let mut missing = input("Missing", ScriptKind::Pre, None);
    missing.spec.program = "Z:/definitely/not/here.exe".into();
    let script = s.scripts.save(&missing).unwrap();
    assert!(
        s.scripts
            .run(&script, s.game, None, ScriptKind::Pre, None)
            .is_err()
    );
}

/// Runs a draft from the script dialog and collects its output until it ends.
fn debug_run(
    s: &Setup,
    spec: ScriptSpec,
    stop: bool,
) -> (String, String, Option<i32>, Option<String>) {
    let (tx, rx) = mpsc::channel();
    let run = Uuid::new_v4();
    s.events.subscribe(move |event| match event {
        CoreEvent::ScriptOutput { run: r, .. } | CoreEvent::ScriptRunEnded { run: r, .. }
            if *r == run =>
        {
            let _ = tx.send(event.clone());
        }
        _ => {}
    });
    let mut i = input("Debug", ScriptKind::Pre, None);
    i.spec = spec;
    s.scripts.run_draft(&i, s.game, run).unwrap();
    if stop {
        let started = Instant::now();
        while !s.scripts.stop_run(run) {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the run never started"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let (mut out, mut err) = (String::new(), String::new());
    loop {
        match rx.recv_timeout(Duration::from_secs(20)).unwrap() {
            CoreEvent::ScriptOutput { stream, text, .. } => match stream {
                OutputStream::Stdout => out.push_str(&text),
                OutputStream::Stderr => err.push_str(&text),
            },
            CoreEvent::ScriptRunEnded {
                exit_code, error, ..
            } => return (out, err, exit_code, error),
            _ => unreachable!(),
        }
    }
}

#[test]
fn debug_runs_report_their_output_and_exit_code() {
    let s = setup();
    let command = if cfg!(windows) {
        "echo %QK_GAME_NAME% & echo oops 1>&2 & exit 4"
    } else {
        "echo $QK_GAME_NAME; echo oops 1>&2; exit 4"
    };
    let (out, err, code, error) = debug_run(&s, shell(command), false);
    assert_eq!(out.trim(), "Portal 2");
    assert_eq!(err.trim(), "oops");
    assert_eq!(code, Some(4));
    assert_eq!(error, None);
    // Failures of debug runs show in the dialog, not in the bell.
    assert!(s.notifications.list().is_empty());

    let missing = ScriptSpec {
        program: "Z:/definitely/not/here.exe".into(),
        ..ScriptSpec::default()
    };
    let (_, _, code, error) = debug_run(&s, missing, false);
    assert_eq!(code, None);
    assert!(error.is_some_and(|e| e.contains("could not start")));
}

#[test]
fn debug_runs_can_be_stopped() {
    let s = setup();
    let command = if cfg!(windows) {
        "ping -n 6 127.0.0.1 > nul"
    } else {
        "sleep 5"
    };
    let started = Instant::now();
    let (_, _, code, error) = debug_run(&s, shell(command), true);
    assert_eq!(code, None);
    assert!(error.is_some_and(|e| e.contains("stopped")));
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn finds_the_processes_a_script_started() {
    let proc = |pid, parent| crate::launcher::ProcInfo {
        pid,
        parent,
        exe: None,
        name: String::new(),
        hint_paths: Vec::new(),
    };
    let all = [
        proc(1, None),
        proc(10, Some(1)),
        proc(11, Some(10)),
        proc(12, Some(11)),
        proc(13, Some(10)),
        proc(20, Some(1)),
    ];
    let mut tree = descendants(10, &all);
    tree.sort_unstable();
    assert_eq!(tree, [11, 12, 13]);
    assert!(descendants(99, &all).is_empty());
}
