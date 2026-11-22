use super::*;

#[test]
fn repeated_subscribe_reuses_one_native_watcher_and_one_owner() {
    let fixture = tempfile::tempdir().unwrap();
    let (service, _receiver) = WatcherService::new();
    for _ in 0..MAX_WATCHERS + 1 {
        service.subscribe(fixture.path(), 1).unwrap();
    }
    assert_eq!(service.active_watchers.load(Ordering::SeqCst), 1);
    service.unsubscribe(fixture.path(), 1);
    assert_eq!(service.active_watchers.load(Ordering::SeqCst), 0);
    assert!(service.subscriptions.lock().unwrap().is_empty());
}

#[test]
fn concurrent_windows_share_native_watcher_until_last_owner_leaves() {
    let fixture = tempfile::tempdir().unwrap();
    let (service, _receiver) = WatcherService::new();
    thread::scope(|scope| {
        for window_id in 1..=8 {
            let service = &service;
            let root = fixture.path();
            scope.spawn(move || service.subscribe(root, window_id).unwrap());
        }
    });
    assert_eq!(service.active_watchers.load(Ordering::SeqCst), 1);
    service.unsubscribe(fixture.path(), 99);
    for window_id in 1..8 {
        service.unsubscribe(fixture.path(), window_id);
        assert_eq!(service.active_watchers.load(Ordering::SeqCst), 1);
    }
    service.unsubscribe(fixture.path(), 8);
    assert_eq!(service.active_watchers.load(Ordering::SeqCst), 0);
    service.subscribe(fixture.path(), 9).unwrap();
    assert_eq!(service.active_watchers.load(Ordering::SeqCst), 1);
}

#[test]
fn dropping_service_stops_native_pumps_and_releases_batch_senders() {
    let fixture = tempfile::tempdir().unwrap();
    let (service, receiver) = WatcherService::new();
    service.subscribe(fixture.path(), 1).unwrap();
    let active = Arc::clone(&service.active_watchers);
    drop(service);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(1)),
        Err(crossbeam_channel::RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn disconnected_shutdown_sender_terminates_pump() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().to_owned();
    let (notify_tx, notify_rx) = crossbeam_channel::unbounded();
    let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded(1);
    let (emitter, _receiver) = crossbeam_channel::unbounded();
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    drop(shutdown_tx);
    let worker = thread::spawn(move || {
        pump(
            &root,
            "root".into(),
            &notify_rx,
            &Arc::new(Mutex::new(HashMap::new())),
            &emitter,
            BatchWindow::default(),
            &mut Some(shutdown_rx),
        );
        done_tx.send(()).unwrap();
    });
    let result = done_rx.recv_timeout(Duration::from_secs(1));
    // Disconnect notify too, so even the regressed implementation can tear down.
    drop(notify_tx);
    worker.join().unwrap();
    assert!(
        result.is_ok(),
        "shutdown disconnect must stop a live notify pump"
    );
}
