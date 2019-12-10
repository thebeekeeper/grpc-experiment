//#![deny(warnings, rust_2018_idioms)]

use crate::hello_world::{server, HelloReply, HelloRequest};

use futures::future::lazy;
use std::sync::{Arc, Mutex};
use futures::{future, Future, Stream};
use log::error;
use tokio::net::TcpListener;
use tower_grpc::{Request, Response};
use tower_hyper::server::{Http, Server};
use tokio::io;
use std::io::BufReader;

pub mod hello_world {
    include!(concat!(env!("OUT_DIR"), "/helloworld.rs"));
}

#[derive(Clone, Debug)]
struct Greet {
    // wrapping this in arc<mutex> makes it so it can be cloned, and we can access it across tasks
    state: Arc<Mutex<State>>
}

#[derive(Debug)]
struct State {
    pub switch_value: i32,
}

impl server::Greeter for Greet {
    type SayHelloFuture = future::FutureResult<Response<HelloReply>, tower_grpc::Status>;

    // all we need to do here is respond with the current switch value
    fn say_hello(&mut self, _request: Request<HelloRequest>) -> Self::SayHelloFuture {
        //println!("REQUEST = {:?}", request);
        let state = self.state.lock().unwrap();
        let response = Response::new(HelloReply {
            value: state.switch_value,
        });

        future::ok(response)
    }
}

pub fn main() {
    let _ = ::env_logger::init();

    let greet = Greet { state: Arc::new(Mutex::new(
        State {
            switch_value: 100,
        }
    ))};
    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let lines = io::lines(reader);
    // clone the arc mutex so we can pass it into the closure
    let g = greet.state.clone();
    let fut = lines
        .for_each(move |line| {
            let mut gg = g.lock().unwrap();
            if line.starts_with("u") {
                gg.switch_value = 500;
            } else {
                gg.switch_value = 100;
            }
            println!("Set switch value to: {}", gg.switch_value);
            Ok(())
        })
        .map_err(|e| eprintln!("error reading from stdin: {:?}", e));
    let new_service = server::GreeterServer::new(greet);
    let mut server = Server::new(new_service);

    let http = Http::new().http2_only(true).clone();

    let addr = "[::1]:50051".parse().unwrap();
    let bind = TcpListener::bind(&addr).expect("bind");

    let serve = bind
        .incoming()
        .for_each(move |sock| {
            if let Err(e) = sock.set_nodelay(true) {
                return Err(e);
            }

            let serve = server.serve_with(sock, http.clone());
            tokio::spawn(serve.map_err(|e| error!("hyper error: {:?}", e)));

            Ok(())
        })
        .map_err(|e| eprintln!("accept error: {}", e));

    tokio::run(lazy(|| {
        tokio::spawn(lazy(move || {
            serve
        }
        ));
        tokio::spawn(lazy(move || {
            println!("Press U to set switch value high");
            fut
        }));
        Ok(())
    }));
}
