use hyper::body::Bytes;
use hyper::header::HeaderValue;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Method, Request, Response, Server, StatusCode};
use secstr::SecStr;

use std::env;
use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};

use hmac::{Hmac, Mac};
use sha2::Sha256;

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

fn verify_gh_sig(signature: &HeaderValue, body: &Bytes) -> bool {
    let key = env::var("GITHUB_WEBHOOK_TOKEN").unwrap();
    let mut hmac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("");
    // without expect(""), the thing breaks, but the string that you pass doesn't matter...

    hmac.update(body);

    let res = hmac.finalize();

    let sig = match signature.to_str() {
        Ok(str) => str,
        Err(_) => "",
    };

    let resbytes = res.into_bytes();
    println!("sha256={:x}", resbytes);
    println!("{}", sig);
    SecStr::from(format!("sha256={:x}", resbytes)) == SecStr::from(format!("{}", sig))
}

/* Starts to run a new instance of the program if the Pid is -1 otherwise does nothing.  */
async fn up(pid: i32) -> i32 {
    match pid {
        -1 => {
            let child = Some(
                Command::new("python3")
                    .current_dir("/home/benlubas/github/test-program/")
                    .arg("print-forever.py")
                    .spawn()
                    .expect("problem with the spawned progam"),
            );
            let new_pid: i32 = match child {
                Some(child) => (child.id() as i32),
                None => -1,
            };
            println!("Spawned child thread with pid: {new_pid}");
            new_pid
        }
        _ => -1,
    }
}

/* Kills the program with the specified Pid, returns false if it failed to
kill the program. returns true if the pid is -1 */
async fn down(pid: i32) -> bool {
    match pid {
        -1 => true,
        _ => {
            let mut kill = Command::new("kill")
                .arg(format!("{pid}"))
                .spawn()
                .expect("problem killing program");

            match kill.wait() {
                Ok(_) => {
                    println!("Killed child program with pid: {pid}");
                    true
                }
                Err(_) => false,
            }
        }
    }
}

type Pid = Arc<RwLock<i32>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = ([127, 0, 0, 1], 3456).into();

    let arc_pid: Pid = Arc::new(RwLock::new(-1));

    let service = make_service_fn(move |_| {
        let arc_pid = arc_pid.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                let arc_pid = arc_pid.clone();
                async move {
                    match (req.method(), req.uri().path()) {
                        // Serve some instructions at /
                        (&Method::GET, "/") => Ok::<_, Error>(Response::new(Body::from(
                            "this only exists to rebuild the Jerry-rs bot on a github deploy.\
                            \nTo manually rebuild (without pulling), send POST req to /rebuild",
                        ))),

                        (&Method::POST, "/github") => {
                            println!("incomming github even");
                            let (parts, body) = req.into_parts();
                            let signature = parts.headers.get("x-hub-signature-256").unwrap();
                            let body = hyper::body::to_bytes(body).await?;
                            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

                            if verify_gh_sig(signature, &body) {
                                println!("now we can down the thing, pull the stuff, and run the thing again.");
                            }
                            Ok::<_, Error>(Response::default())
                        }

                        (&Method::POST, "/up") => {
                            let pid = arc_pid.read().unwrap().clone();
                            let new_pid = up(pid).await;
                            let mut pid = arc_pid.write().unwrap();
                            *pid = new_pid;

                            Ok::<_, Error>(Response::default())
                        }

                        (&Method::POST, "/down") => {
                            let pid = arc_pid.read().unwrap().clone();
                            match down(pid).await {
                                true => {
                                    let mut pid = arc_pid.write().unwrap();
                                    *pid = -1;
                                    Ok::<_, Error>(Response::default())
                                }
                                false => Ok::<_, Error>(
                                    Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .body(Body::from(""))
                                        .unwrap(),
                                ),
                            }
                        }

                        // Return the 404 Not Found for other routes.
                        _ => {
                            let mut not_found = Response::default();
                            *not_found.status_mut() = StatusCode::NOT_FOUND;
                            Ok::<_, Error>(not_found)
                        }
                    }
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(service);

    let graceful = server.with_graceful_shutdown(shutdown_signal());

    println!("Listening on http://{}", addr);

    if let Err(e) = graceful.await {
        eprintln!("server error: {}", e);
    }

    Ok(())
}
