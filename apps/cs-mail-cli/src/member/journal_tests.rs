use super::*;
use std::io::{BufRead, BufReader, Read, Write};

// Claim: an ambiguous response, including authorization lost after an earlier commit,
// cannot create a new operation. A different body after restarting the client falsifies
// it and could cause a second email or charge. Exercise the actual HTTP and disk journal.
#[test]
fn transport_errors_preserve_the_operation_across_restart() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let server = format!("http://{}/", listener.local_addr().unwrap());
    let host = std::thread::spawn(move || {
        let mut operations = std::collections::BTreeSet::new();
        for (status, answer) in [
            ("503 Service Unavailable", r#"{"error":"unavailable"}"#),
            ("401 Unauthorized", r#"{"error":"unauthorized"}"#),
            ("200 OK", "unreadable"),
            ("200 OK", r#"{"committed":true}"#),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut length = None;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let mut bytes = vec![0; length.unwrap()];
            reader.read_exact(&mut bytes).unwrap();
            // The first operation committed even though its response says unavailable.
            operations.insert(bytes);
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len()).unwrap();
        }
        operations.len()
    });
    let home = std::env::temp_dir().join(format!("cs-mail-journal-{}", identifier().unwrap()));
    init(&home, server).unwrap();
    let mut member = Member::open(&home).unwrap();
    assert!(
        member
            .journaled(
                "send",
                "v1/external/commands",
                |_| {
                    Ok(json!({"operation": identifier()?.to_string(), "ciphertext": [1, 2, 3]}))
                },
                false
            )
            .is_err()
    );
    drop(member);
    for attempt in 0..3 {
        let mut member = Member::open(&home).unwrap();
        let answer = member.journaled(
            "send",
            "v1/external/commands",
            |_| panic!("an uncertain operation must not be rebuilt or signed again"),
            false,
        );
        assert_eq!(answer.is_ok(), attempt == 2);
    }
    assert_eq!(host.join().unwrap(), 1);
    let mut member = Member::open(&home).unwrap();
    assert_eq!(
        member
            .journaled(
                "send",
                "v1/external/commands",
                |_| panic!("already answered"),
                false
            )
            .unwrap(),
        json!({"committed": true})
    );
    std::fs::remove_dir_all(home).unwrap();
}
