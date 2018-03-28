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

use hyper::{Method, StatusCode};
use hyper::server::{Http, Request, Response, Service};

use prometheus::{Encoder, Gauge, TextEncoder, Registry, Opts};

use clap::{Arg, App};

use slog::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Port {
    name: String,
    port: u64
}

#[derive(Clone)]
struct Entry {
    gauge: Gauge,
    port: Port
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    ports: Vec<Port>
}

#[derive(Clone)]
struct Exporter {
    metrics: Vec<Entry>,
    logger: slog::Logger,
    registry: Registry
}

impl Service for Exporter {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;

    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let mut response = Response::new();

        match (req.method(), req.path()) {
            (&Method::Get, "/") => {
                response.set_body("<html>
             <head><title>Marathon Exporter</title></head>
             <body>
             <h1>Marathon Exporter</h1>
             <p><a href='9099'>Metrics</a></p>
             </body>
             </html>`");
                return Box::new(futures::future::ok(response));
            },
            (&Method::Get, "/metrics") => {
              for port in &self.metrics  {
                  match TcpStream::connect("127.0.0.1:".to_string() + &port.port.port.to_string()) {
                    Ok(_) => port.gauge.set(1.0),
                    Err(_) => {
                        error!(self.logger, "Error connecting to"; "service" => port.port.name.clone(), "port" => port.port.port.clone());
                        port.gauge.set(0.0);
                    }
                };  
              } 
            },
            _ => {
                response.set_status(StatusCode::NotFound);
            },
        }

        let mut buffer = Vec::<u8>::new();
        let encoder = TextEncoder::new();
        let metrics_familys = self.registry.gather();
        encoder.encode(&metrics_familys, &mut buffer).unwrap();

        response.set_body(buffer);

        return Box::new(futures::future::ok(response));
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
    let registry = Registry::new();

    let mut gauges = Vec::new();

    for port in config.ports {
        let opts = Opts::new("proccess_up", "check if port is up")
            .const_label("name", &port.name)
            .const_label("port", &port.port.to_string());
        let gauge = Gauge::with_opts(opts).unwrap();
        registry.register(Box::new(gauge.clone())).unwrap();

        gauges.push(Entry {port: port, gauge: gauge});
    }

    let address = "0.0.0.0:".to_string() + port;
    let exporter = Exporter {metrics: gauges, logger: logger, registry: registry};
    let server = Http::new().bind(&address.parse().unwrap(), move || Ok(exporter.clone())).unwrap();
    server.run().unwrap();
}
