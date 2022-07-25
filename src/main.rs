use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Method, Request, Response, Server, StatusCode};

use std::process::Command;
use std::sync::{Arc, Mutex};

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

type Pid = Arc<Mutex<i32>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = ([127, 0, 0, 1], 3456).into();

    let arc_pid: Pid = Arc::new(Mutex::new(-1));

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

                        (&Method::POST, "/up") => {
                            let mut pid = arc_pid.lock().unwrap();
                            match *pid {
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
                                    *pid = new_pid;
                                    Ok::<_, Error>(Response::default())
                                }
                                _ => Ok::<_, Error>(Response::default()),
                            }
                        }

                        (&Method::POST, "/down") => {
                            let mut pid = arc_pid.lock().unwrap();
                            match *pid {
                                -1 => Ok::<_, Error>(Response::default()),
                                _ => {
                                    let mut kill = Command::new("kill")
                                        .arg(format!("{pid}"))
                                        .spawn()
                                        .expect("problem killing program");

                                    match kill.wait() {
                                        Ok(_) => {
                                            println!("Killed child program with pid: {pid}");
                                            *pid = -1;
                                            Ok::<_, Error>(Response::default())
                                        }
                                        Err(_) => Ok::<_, Error>(
                                            Response::builder()
                                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                                .body(Body::from(""))
                                                .unwrap(),
                                        ),
                                    }
                                }
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
