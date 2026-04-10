use super::*;

#[test]
fn totals_starts_at_zero() {
    let totals = ConnectionTotals::default();
    let traffic = totals.settled_totals();
    assert_eq!(traffic.upload_bytes, 0);
    assert_eq!(traffic.download_bytes, 0);
}

#[test]
fn totals_record_finished_accumulates() {
    let totals = ConnectionTotals::default();
    let t1 = ConnectionTraffic {
        upload_bytes: 100,
        download_bytes: 200,
    };
    let settled = totals.record_finished_connection(t1);
    assert_eq!(settled.upload_bytes, 100);
    assert_eq!(settled.download_bytes, 200);

    let t2 = ConnectionTraffic {
        upload_bytes: 300,
        download_bytes: 400,
    };
    let settled = totals.record_finished_connection(t2);
    assert_eq!(settled.upload_bytes, 400);
    assert_eq!(settled.download_bytes, 600);
}

#[test]
fn totals_settled_totals_matches_record() {
    let totals = ConnectionTotals::default();
    totals.record_finished_connection(ConnectionTraffic {
        upload_bytes: 50,
        download_bytes: 75,
    });
    totals.record_finished_connection(ConnectionTraffic {
        upload_bytes: 25,
        download_bytes: 125,
    });
    let traffic = totals.settled_totals();
    assert_eq!(traffic.upload_bytes, 75);
    assert_eq!(traffic.download_bytes, 200);
    assert_eq!(traffic.total_bytes(), 275);
}

#[test]
fn totals_record_finished_returns_cumulative() {
    let totals = ConnectionTotals::default();
    let result = totals.record_finished_connection(ConnectionTraffic {
        upload_bytes: 10,
        download_bytes: 20,
    });
    assert_eq!(result.upload_bytes, 10);
    assert_eq!(result.download_bytes, 20);

    let result = totals.record_finished_connection(ConnectionTraffic {
        upload_bytes: 30,
        download_bytes: 40,
    });
    assert_eq!(result.upload_bytes, 40);
    assert_eq!(result.download_bytes, 60);
}

#[test]
fn totals_clone_shares_state() {
    let totals = ConnectionTotals::default();
    let clone = totals.clone();
    totals.record_finished_connection(ConnectionTraffic {
        upload_bytes: 999,
        download_bytes: 1,
    });
    let traffic = clone.settled_totals();
    assert_eq!(traffic.upload_bytes, 999);
    assert_eq!(traffic.download_bytes, 1);
}

#[test]
fn totals_concurrent_record() {
    let totals = Arc::new(ConnectionTotals::default());
    let mut handles = Vec::new();
    for i in 0..4 {
        let t = Arc::clone(&totals);
        handles.push(std::thread::spawn(move || {
            t.record_finished_connection(ConnectionTraffic {
                upload_bytes: (i + 1) as u64 * 100,
                download_bytes: (i + 1) as u64 * 50,
            });
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let traffic = totals.settled_totals();
    assert_eq!(traffic.upload_bytes, 100 + 200 + 300 + 400);
    assert_eq!(traffic.download_bytes, 50 + 100 + 150 + 200);
}
