use island_events::{Server, Store};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
fn start() -> Server {
    Server::bind(
        "127.0.0.1:0".parse().unwrap(),
        TOKEN.into(),
        Arc::new(Mutex::new(Store::default())),
        Arc::new(|| {}),
    )
    .unwrap()
}
fn exchange(
    server: &Server,
    method: &str,
    path: &str,
    body: &str,
    extra: &str,
) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(server.address()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}\r\n{body}", server.address(), body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    (
        headers.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}
fn auth() -> String {
    format!("Authorization: Bearer {TOKEN}\r\n")
}

#[test]
fn authenticated_clients_can_create_update_query_and_delete() {
    let server = start();
    let (code, value) = exchange(
        &server,
        "POST",
        "/activity",
        r#"{"id":"build","title":"构建","progress":0.25}"#,
        &auth(),
    );
    assert_eq!(code, 200);
    assert_eq!(value["id"], "build");
    assert_eq!(
        exchange(
            &server,
            "POST",
            "/activity",
            r#"{"id":"build","title":"构建","progress":0.75}"#,
            &auth()
        )
        .0,
        200
    );
    let (_, value) = exchange(&server, "GET", "/status", "", &auth());
    assert_eq!(value["activities"].as_array().unwrap().len(), 1);
    assert_eq!(value["activities"][0]["progress"], 0.75);
    assert_eq!(
        exchange(&server, "POST", "/notify", r#"{"title":"完成"}"#, &auth()).0,
        200
    );
    assert_eq!(
        exchange(&server, "DELETE", "/activity/build", "", &auth()).0,
        200
    );
    assert_eq!(
        exchange(&server, "DELETE", "/activity/build", "", &auth()).0,
        404
    );
}

#[test]
fn missing_or_wrong_credentials_and_browser_origins_are_rejected() {
    let server = start();
    assert_eq!(exchange(&server, "GET", "/status", "", "").0, 401);
    assert_eq!(
        exchange(
            &server,
            "GET",
            "/status",
            "",
            "Authorization: Bearer wrong\r\n"
        )
        .0,
        401
    );
    assert_eq!(
        exchange(
            &server,
            "GET",
            "/status",
            "",
            &format!("{}Origin: https://example.com\r\n", auth())
        )
        .0,
        403
    );
    assert!(Server::bind(
        "0.0.0.0:0".parse::<SocketAddr>().unwrap(),
        TOKEN.into(),
        Arc::new(Mutex::new(Store::default())),
        Arc::new(|| {})
    )
    .is_err());
}

#[test]
fn bad_json_large_bodies_and_conflicting_http_framing_are_rejected() {
    let server = start();
    assert_eq!(exchange(&server, "POST", "/activity", "{", &auth()).0, 400);
    assert_eq!(
        exchange(
            &server,
            "POST",
            "/activity",
            r#"{"title":"bad","progress":2}"#,
            &auth()
        )
        .0,
        400
    );
    assert_eq!(
        exchange(&server, "POST", "/activity", &"x".repeat(33000), &auth()).0,
        413
    );
    assert_eq!(
        exchange(
            &server,
            "POST",
            "/activity",
            "{}",
            &format!("{}Transfer-Encoding: chunked\r\n", auth())
        )
        .0,
        400
    );
    assert_eq!(
        exchange(
            &server,
            "POST",
            "/activity",
            "{}",
            &format!("{}Content-Length: 2\r\n", auth())
        )
        .0,
        400
    );
    assert_eq!(exchange(&server, "GET", "/unknown", "", &auth()).0, 404);
}

#[test]
fn stopping_the_service_releases_the_port() {
    let server = start();
    let address = server.address();
    drop(server);
    let listener = std::net::TcpListener::bind(address).unwrap();
    drop(listener);
}
