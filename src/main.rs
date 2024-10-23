// Copyright 2019-2021 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std::net::SocketAddr;
use std::time::Duration;

use hyper::body::Bytes;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::rpc_params;
use jsonrpsee::server::{RpcModule, Server};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing_subscriber::util::SubscriberInitExt;

use serde::{Deserialize, Serialize};
// use serde_json::Value;

use ark_bn254::Fr;
use ark_ff::{Field, PrimeField};
use std::str::FromStr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()?
        .add_directive("jsonrpsee[method_call{name = \"say_hello\"}]=trace".parse()?);
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(filter)
        .finish()
        .try_init()?;

    let server_addr = run_server().await?;

    // MAYBE WE DON'T NEED ANY OF THIS STUFF BELOW!??
    let url = format!("http://{}", server_addr);

    let middleware = tower::ServiceBuilder::new()
	.layer(
		TraceLayer::new_for_http()
			.on_request(
				|request: &hyper::Request<_>, _span: &tracing::Span| tracing::info!(request = ?request, "on_request"),
			)
			.on_body_chunk(|chunk: &Bytes, latency: Duration, _: &tracing::Span| {
				tracing::info!(size_bytes = chunk.len(), latency = ?latency, "sending body chunk")
			})
			.make_span_with(DefaultMakeSpan::new().include_headers(true))
			.on_response(DefaultOnResponse::new().include_headers(true).latency_unit(LatencyUnit::Micros)),
	);

    let client = HttpClient::builder()
        .set_http_middleware(middleware)
        .build(url)?;

    // Try a request:
    let params = rpc_params![1_u64, 2, 3];
    let response: Result<String, _> = client.request("say_hello", params).await;
    tracing::info!("r: {:?}", response);

    Ok(())
}

// THIS IS A TYPESCRIPT EXAMPLE
// server.addMethod("resolve_foreign_call", async (params) => {
//     if (params[0].function !== "getSqrt") {
//         throw Error("Unexpected foreign call")
//     };
//     const values = params[0].inputs[0].map((field) => {
//         return `${Math.sqrt(parseInt(field, 16))}`;
//     });
//     return { values: [values] };
// });

fn print_type<T>(_: &T) {
    println!("{:?}", std::any::type_name::<T>());
}

#[derive(Debug, Deserialize)]
struct RequestData {
    session_id: u64,
    function: String,
    inputs: Vec<String>,
    root_path: String,
    package_name: String,
}
#[derive(Debug, Deserialize)]
struct Requests(Vec<RequestData>); // Wrap it in a struct to handle the array
fn handle_get_sqrt(inputs: &Vec<String>) -> String {
    println!("inputs: {:?}", inputs[0]);

    let inputs_str = inputs[0].as_str().trim_start_matches('0'); // Trimming leading zeroes turned out to be very important, otherwise `from_str` on the next line was erroring!
    println!("inputs_str: {:?}", inputs_str);

    let x: Fr = Fr::from_str(inputs_str).unwrap();

    println!("x: {:?}", x);

    // SQRT CODE COPIED FROM ARKWORKS README:
    // We can check if a field element is a square by computing its Legendre symbol...
    let sqrt = if x.legendre().is_qr() {
        // ... and if it is, we can compute its square root.
        let sqrt = x.sqrt().unwrap();
        assert_eq!(sqrt.square(), x);

        Some(sqrt)
    } else {
        // Otherwise, we can check that the square root is `None`.
        assert_eq!(x.sqrt(), None);

        None
    };

    println!("Computed sqrt: {:?}", sqrt);

    if sqrt == None {
        // I can't be bothered figuring out how to serialise an `Option::None`, so I'm panicking in this case, instead.
        panic!("division by zero");
    }

    sqrt.unwrap().into_bigint().to_string()
}

fn handle_unknown_function(input: &RequestData) -> String {
    println!("oops");
    String::from("oops")
}

async fn run_server() -> anyhow::Result<SocketAddr> {
    let server = Server::builder()
        .build("127.0.0.1:3000".parse::<SocketAddr>()?)
        .await?;
    let mut module = RpcModule::new(());

    module.register_method("say_hello", |_, _, _| "lo")?;

    module.register_method("resolve_foreign_call", |params, _, _| {
        print_type(&params);
        println!("params{:?}", params);

        // Attempt to extract the string from Params
        let response: String = if let Some(json_string) = params.as_str() {
            // Deserialize the JSON string into your struct
            let requests: Requests =
                serde_json::from_str(&json_string).expect("Failed to parse JSON");

            let request = &requests.0[0];

            let result: String = match request.function.as_str() {
                "getSqrt" => handle_get_sqrt(&request.inputs),
                // "getSum" => handle_get_sum(&request),
                // "getDiff" => handle_get_diff(&request),
                _ => handle_unknown_function(&request),
            };
            println!("{:?}", request.function);
            println!("result: {:?}", result);
            result
        } else {
            println!("No parameters provided");
            String::from("Bad query")
        };

        println!("response: {:?}", response);

        response
    })?;

    let addr = server.local_addr()?;
    let handle = server.start(module);

    println!("Server is running on 127.0.0.1:3000");

    // In this example we don't care about doing shutdown so let's it run forever.
    // You may use the `ServerHandle` to shut it down or manage it yourself.
    // tokio::spawn(handle.stopped());

    // Keep the server running until it's interrupted
    handle.stopped().await;

    Ok(addr)
}
