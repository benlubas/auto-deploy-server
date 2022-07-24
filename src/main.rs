use std::env;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use sha2::Sha256;
use hmac::{Hmac, Mac};

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn echo(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        // Serve some instructions at /
        (&Method::GET, "/") => Ok(Response::new(Body::from(
            "this only exists to rebuild the Jerry-rs bot on a github deploy.\
            \nTo manually rebuild (without pulling), send POST req to /rebuild"
        ))),

        // rebuild on git push
        (&Method::POST, "/payload") => {
            println!("req: \n {:?} ", req);
            let (parts, body) = req.into_parts();
            let signature = parts.headers.get("x-hub-signature-256").unwrap();
            let body = hyper::body::to_bytes(body).await?;
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

            let key = env::var("GITHUB_WEBHOOK_TOKEN").unwrap();
            let mut hmac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
                .expect(""); 
                // without this line, the thing breaks, but the string that you pass doesn't matter...

            hmac.update(&body);

            let res = hmac.finalize();

            // let key: GenericArray<u8, U64> = GenericArray::from_iter(key.into_bytes());
            // let mut hmac: HmacCore<Sha256> = Hmac::new_from_slice(&key.as_bytes());
            // let hash = hmac.update(&body);
            

            println!("{:?}", json);
            println!("{:x}", res.into_bytes());
            println!("sha256={:?}", key);
            println!("{:?}", signature);
            
            Ok(Response::default())
        }

        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

// fn verify_signature(payload_body: Request<Body>) -> Result<bool, hyper::Error> {
//     let hash = Sha256::digest("sha256=");
// }

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = ([127, 0, 0, 1], 3456).into();

    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(echo)) });

    let server = Server::bind(&addr).serve(service);

    let graceful = server.with_graceful_shutdown(shutdown_signal());

    println!("Listening on http://{}", addr);

    if let Err(e) = graceful.await {
        eprintln!("server error: {}", e);
    }

    Ok(())
}
