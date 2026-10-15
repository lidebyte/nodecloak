use super::*;
use std::net::TcpListener;

#[test]
fn disconnect_keeps_transport_diagnostic_instead_of_erasing_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        ws.close(None).unwrap();
    });
    let (socket, _) = tungstenite::connect(format!("ws://{address}")).unwrap();
    let mut protocol = Protocol {
        socket,
        next: 0,
        events: VecDeque::new(),
        last_error: None,
        navigation: None,
        reconnect_endpoint: None,
    };
    let error = protocol.read().unwrap_err();
    server.join().unwrap();
    assert!(
        error.contains("WebSocket Close"),
        "lost transport diagnostic: {error}"
    );
}
