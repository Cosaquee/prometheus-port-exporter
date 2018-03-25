extern crate hyper;
extern crate futures;
extern crate prometheus;
extern crate clap;

#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;

#[macro_use]
extern crate slog;
extern crate slog_term;

use std::io::prelude::*;
use std::fs::File;
use std::net::TcpStream;

use futures::future::Future;

use hyper::header::ContentLength;
use hyper::server::{Http, Request, Response, Service};

use prometheus::{Encoder, Gauge, TextEncoder, Registry, Opts};

use clap::{Arg, App};

use slog::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Port {
    name: String,
    port: u64
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    ports: Vec<Port>
}

#[derive(Clone)]
struct Exporter {
    config: Config,
    logger: slog::Logger
}

impl Service for Exporter {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;

    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, _req: Request) -> Self::Future {
        let registry = Registry::new();

        for port in &self.config.ports {
            let opts = Opts::new("proccess_up", "check if port is up")
                .const_label("name", &port.name)
                .const_label("port", &port.port.to_string());
            let gauge = Gauge::with_opts(opts).unwrap();
            registry.register(Box::new(gauge.clone())).unwrap();

            match TcpStream::connect("127.0.0.1:".to_string() + &port.port.to_string()) {
                Ok(_) => gauge.set(1.0),
                Err(_) => {
                    error!(self.logger, "Error connecting to"; "service" => port.name.clone(), "port" => port.port.clone());
                    gauge.set(0.0);
                }
            };  
        }

        let mut buffer = Vec::<u8>::new();
        let encoder = TextEncoder::new();
        let metrics_familys = registry.gather();
        encoder.encode(&metrics_familys, &mut buffer).unwrap();

        Box::new(futures::future::ok(
            Response::new()
            .with_header(ContentLength(buffer.len() as u64))
            .with_body(buffer)
        ))
    }
}

fn main() {
    let matches = App::new("Prometheus Port exporter")
        .version("0.1.0")
        .author("Karol Kozakowski <cosaquee@gmail.com>")
        .about("Monitor if port accepts connections")
        .arg(Arg::with_name("config")
            .short("c")
            .long("config")
            .help("path to configuration file")
            .takes_value(true))
        .arg(Arg::with_name("port")
            .short("p")
            .long("port")
            .value_name("PORT")
            .help("Port for exporter to expose")
            .takes_value(true))
        .get_matches();

    let decorator = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let drain = slog_term::FullFormat::new(decorator).build().fuse();

    let logger = slog::Logger::root(drain, o!());

    let config_file = matches.value_of("config").unwrap_or("/opt/port_exporter.yaml");
    let port = matches.value_of("port").unwrap_or("9099");

    info!(logger, "Prometheus Port exporter started"; "port" => port );

    let mut file = File::open(config_file).expect("Unable to open the file");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("Unable to read the file");

    let config : Config = serde_yaml::from_str(&contents).unwrap();

    let address = "127.0.0.1:".to_string() + port;
    let exporter = Exporter {config: config, logger: logger};
    let server = Http::new().bind(&address.parse().unwrap(), move || Ok(exporter.clone())).unwrap();
    server.run().unwrap();
}
