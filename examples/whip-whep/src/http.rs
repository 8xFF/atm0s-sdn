use std::io::Read;
use std::{collections::HashMap, fs::File, net::SocketAddr, path::Path, time::Duration};
use tiny_http::{Header, Method, Request, Response, Server};

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub req_id: u64,
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn http_auth(&self) -> String {
        if let Some(auth) = self.headers.get("Authorization") {
            auth.clone()
        } else if let Some(auth) = self.headers.get("authorization") {
            auth.clone()
        } else {
            "demo".to_string()
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub req_id: u64,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub struct SimpleHttpServer {
    req_id_seed: u64,
    server: Server,
    reqs: HashMap<u64, Request>,
}

impl SimpleHttpServer {
    pub fn new(port: u16) -> Self {
        Self {
            req_id_seed: 0,
            server: Server::http(SocketAddr::from(([0, 0, 0, 0], port))).expect("Should open http port"),
            reqs: HashMap::new(),
        }
    }

    pub fn send_response(&mut self, res: HttpResponse) {
        log::info!("sending response for request_id {}, status {}", res.req_id, res.status);
        let req = self.reqs.remove(&res.req_id).expect("Should have a request.");
        let mut response = Response::from_data(res.body).with_status_code(res.status);
        for (k, v) in res.headers {
            response.add_header(Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap());
        }
        response.add_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
        response.add_header(Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, PATCH, DELETE, OPTIONS").unwrap());
        response.add_header(Header::from_bytes("Access-Control-Allow-Headers", "*").unwrap());
        response.add_header(Header::from_bytes("Access-Control-Allow-Credentials", "true").unwrap());
        req.respond(response).unwrap();
    }

    pub fn recv(&mut self, timeout: Duration) -> Result<Option<HttpRequest>, std::io::Error> {
        let mut request = if let Some(req) = self.server.recv_timeout(timeout)? {
            req
        } else {
            return Ok(None);
        };
        if request.url().starts_with("/public") {
            if let Ok(file) = File::open(&Path::new(&format!(".{}", request.url()))) {
                let mut response = tiny_http::Response::from_file(file);
                if request.url().ends_with(".js") {
                    response.add_header(Header::from_bytes("Content-Type", "application/javascript").unwrap());
                } else if request.url().ends_with(".css") {
                    response.add_header(Header::from_bytes("Content-Type", "text/css").unwrap());
                }
                request.respond(response).expect("Should respond file.");
                return Ok(None);
            } else {
                let response = Response::from_string("Not Found");
                request.respond(response.with_status_code(404)).expect("Should respond 404.");
                return Ok(None);
            }
        }

        if request.method().eq(&Method::Options) {
            let mut response = Response::from_string("OK");
            //setting CORS
            response.add_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
            response.add_header(Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, PATCH, DELETE, OPTIONS").unwrap());
            response.add_header(Header::from_bytes("Access-Control-Allow-Headers", "*").unwrap());
            response.add_header(Header::from_bytes("Access-Control-Allow-Credentials", "true").unwrap());

            request.respond(response).expect("Should respond options.");
            return Ok(None);
        }

        log::info!("received request_id {} method: {}, url: {}", self.req_id_seed, request.method(), request.url(),);

        let req_id = self.req_id_seed;
        self.req_id_seed += 1;

        let res = Ok(Some(HttpRequest {
            req_id,
            method: request.method().to_string(),
            path: request.url().to_string(),
            headers: request.headers().iter().map(|h| (h.field.to_string(), h.value.to_string())).collect(),
            body: request.as_reader().bytes().map(|b| b.unwrap()).collect(),
        }));
        self.reqs.insert(req_id, request);
        res
    }
}
