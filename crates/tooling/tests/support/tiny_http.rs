//! A one-file HTTP server for driving the scripts that call a registry.
//!
//! The scripts under test really run `curl` against a really listening socket —
//! nothing about the layer under test is stubbed. Only the *registry* on the far
//! side is local, which is the one third party these tests cannot run.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

/// A server bound to an ephemeral port, answering every request the same way
/// until it is dropped.
pub struct Server {
    port: u16,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Start a server answering every request with `status` and `body`.
    pub fn answering(status: u16, body: &'static [u8]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
        let port = listener.local_addr().expect("a bound address").port();
        listener
            .set_nonblocking(true)
            .expect("non-blocking listener");
        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = std::sync::Arc::clone(&shutdown);
        let handle = std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => answer(stream, status, body),
                    Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            shutdown,
            handle: Some(handle),
        }
    }

    /// The base URL to hand a script under test.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/crates", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn answer(mut stream: TcpStream, status: u16, body: &[u8]) {
    // Read the request line and headers so the client is not left writing into a
    // closed socket, then answer.
    let mut reader = BufReader::new(stream.try_clone().expect("clone the stream"));
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line == "\r\n" || line == "\n" {
            break;
        }
        line.clear();
    }
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}
