use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use mcode_wire::agent_exec::{ExecParams, LaneRegistry, MAX_OUTPUT_BYTES, cancel, exec};

fn fixture(mode: &str) -> ExecParams {
    ExecParams {
        binary: std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        args: vec![
            "--exact".into(),
            "fixture_subprocess".into(),
            "--nocapture".into(),
        ],
        cwd: Some(
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
        stdin: None,
        timeout_ms: Some(10_000),
        env: Some(BTreeMap::from([(
            "MCODE_EXEC_TEST_MODE".into(),
            mode.into(),
        )])),
        operation: Some(format!("fixture-{mode}")),
    }
}

#[test]
fn fixture_subprocess() {
    let Ok(mode) = std::env::var("MCODE_EXEC_TEST_MODE") else {
        return;
    };
    match mode.as_str() {
        "echo-env-stdin" => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            println!(
                "override={};stdin={input}",
                std::env::var("MCODE_EXEC_TEST_VALUE").unwrap()
            );
        }
        "sleep" | "ignore-stdin" => {
            println!("stdout-before-stop");
            eprintln!("stderr-before-stop");
            std::thread::sleep(Duration::from_secs(8));
        }
        "overflow-stdout" | "overflow-stderr" => {
            let mut stream: Box<dyn Write> = if mode == "overflow-stdout" {
                Box::new(std::io::stdout())
            } else {
                Box::new(std::io::stderr())
            };
            let chunk = [b'x'; 16 * 1024];
            for _ in 0..(MAX_OUTPUT_BYTES / chunk.len() + 8) {
                if stream.write_all(&chunk).is_err() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_secs(8));
        }
        "independent-streams" => {
            std::io::stdout()
                .write_all(&vec![b'o'; MAX_OUTPUT_BYTES / 2 + 1])
                .unwrap();
            std::io::stderr()
                .write_all(&vec![b'e'; MAX_OUTPUT_BYTES / 2 + 1])
                .unwrap();
        }
        "tree" => {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "fixture_subprocess", "--nocapture"])
                .env("MCODE_EXEC_TEST_MODE", "heartbeat")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            let mut descendant = command.spawn().unwrap();
            let _ = descendant.wait();
        }
        "heartbeat" => {
            let path = std::env::var("MCODE_EXEC_TEST_HEARTBEAT").unwrap();
            for tick in 0..200 {
                std::fs::write(&path, tick.to_string()).unwrap();
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        other => panic!("unknown child mode: {other}"),
    }
}

#[tokio::test]
async fn object_environment_and_stdin_reach_the_real_process() {
    let mut params = fixture("echo-env-stdin");
    params.stdin = Some("request input".into());
    params
        .env
        .as_mut()
        .unwrap()
        .insert("MCODE_EXEC_TEST_VALUE".into(), "caller value".into());
    let result = exec(&LaneRegistry::new(), &params, None).await;
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(
        result
            .stdout
            .contains("override=caller value;stdin=request input")
    );
}

#[tokio::test]
async fn live_cancel_interrupts_a_process_and_a_blocked_stdin_write() {
    let registry = LaneRegistry::new();
    let mut params = fixture("ignore-stdin");
    params.stdin = Some("input".repeat(1024 * 1024));
    let started = Instant::now();
    let (result, ()) = tokio::join!(exec(&registry, &params, None), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(cancel(
            &registry,
            params.cwd.as_deref(),
            params.operation.as_deref()
        ));
    });
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(result.canceled, Some(true));
    assert!(!result.timed_out);
    assert!(!cancel(
        &registry,
        params.cwd.as_deref(),
        params.operation.as_deref()
    ));
}

#[tokio::test]
async fn request_abort_interrupts_without_a_lane_and_closed_sender_does_not_cancel() {
    let registry = LaneRegistry::new();
    let mut params = fixture("sleep");
    params.cwd = None;
    let (sender, receiver) = tokio::sync::watch::channel(false);
    let (result, ()) = tokio::join!(exec(&registry, &params, Some(receiver)), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        sender.send_replace(true);
    });
    assert_eq!(result.canceled, Some(true));
    let (sender, receiver) = tokio::sync::watch::channel(false);
    drop(sender);
    let result = exec(
        &LaneRegistry::new(),
        &fixture("independent-streams"),
        Some(receiver),
    )
    .await;
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.canceled, None);
}

#[tokio::test]
async fn superseded_job_finishing_leaves_the_newest_job_cancelable() {
    let registry = LaneRegistry::new();
    let params = fixture("sleep");
    let (old, newest, ()) = tokio::join!(
        exec(&registry, &params, None),
        async {
            tokio::time::sleep(Duration::from_millis(75)).await;
            exec(&registry, &params, None).await
        },
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(cancel(
                &registry,
                params.cwd.as_deref(),
                params.operation.as_deref()
            ));
        }
    );
    assert_eq!(old.canceled, Some(true));
    assert_eq!(newest.canceled, Some(true));
    assert!(!cancel(
        &registry,
        params.cwd.as_deref(),
        params.operation.as_deref()
    ));
}

#[tokio::test]
async fn timeout_preserves_output_already_read() {
    let mut params = fixture("sleep");
    params.timeout_ms = Some(1);
    let result = exec(&LaneRegistry::new(), &params, None).await;
    assert!(result.timed_out);
    assert_eq!(result.exit_code, None);
    assert!(result.stdout.contains("stdout-before-stop"));
    assert!(result.stderr.contains("stderr-before-stop"));
}

#[tokio::test]
async fn each_output_stream_is_bounded_and_overflow_terminates_promptly() {
    for mode in ["overflow-stdout", "overflow-stderr"] {
        let started = Instant::now();
        let result = exec(&LaneRegistry::new(), &fixture(mode), None).await;
        assert!(started.elapsed() < Duration::from_secs(4));
        assert!(!result.timed_out);
        assert_eq!(result.exit_code, None);
        assert!(result.stdout.len() <= MAX_OUTPUT_BYTES);
        assert!(result.stderr.len() <= MAX_OUTPUT_BYTES);
    }
    let result = exec(&LaneRegistry::new(), &fixture("independent-streams"), None).await;
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout.len() + result.stderr.len() > MAX_OUTPUT_BYTES);
}

#[tokio::test]
async fn cancellation_stops_the_descendant_tree() {
    let registry = LaneRegistry::new();
    let mut params = fixture("tree");
    let path = std::env::temp_dir().join(format!(
        "mcode-exec-tree-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    params.env.as_mut().unwrap().insert(
        "MCODE_EXEC_TEST_HEARTBEAT".into(),
        path.to_string_lossy().into_owned(),
    );
    let (result, ()) = tokio::join!(exec(&registry, &params, None), async {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !path.exists() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(path.exists(), "descendant failed to start");
        assert!(cancel(
            &registry,
            params.cwd.as_deref(),
            params.operation.as_deref()
        ));
    });
    assert_eq!(result.canceled, Some(true));
    let after = std::fs::read(&path).unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        std::fs::read(&path).unwrap(),
        after,
        "descendant continued after cancellation"
    );
    std::fs::remove_file(path).unwrap();
}
