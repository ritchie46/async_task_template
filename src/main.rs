#![feature(decl_macro)]
#[macro_use]
extern crate rocket;
use rocket::State;
use std::{future::Future, pin::Pin};
use tokio::{
    runtime::{Builder, Runtime},
    sync::mpsc::{self, Receiver, Sender},
};

/// Send a task to the worker. This code blocks until the message is sent (which should be fast)
fn run_task(task: Task, mut tx: Sender<Task>) {
    let mut rt = simple_async_runtime().unwrap();

    // this blocks only the sending not the execution of the task
    rt.block_on(async move {
        // should not fail as the executing thread should remain alive.
        // note: could not use unwrap() or except() because the Error didn't implement Debug
        assert!((tx.send(task).await.is_ok()));
    });
    println!("task sent")
}

#[get("/start_sync_task")]
fn sync_task_endpoint(tx: State<Sender<Task>>) -> &'static str {
    let some_var = "foo";
    let task = move || {
        println!("start sync task with {}", some_var);
        std::thread::sleep(std::time::Duration::from_secs(3));
        println!("sync task finished");
    };

    run_task(Task::Sync(Box::new(task)), tx.inner().clone());
    "task started"
}

#[get("/start_async_task")]
fn async_task_endpoint(tx: State<Sender<Task>>) -> &'static str {
    let some_var = "foo";

    // we need to help the compiler a bit with the type
    let task: AsyncTask = Box::new(move || {
        Box::pin(async move {
            println!("start async task with {}", some_var);
            tokio::time::delay_for(std::time::Duration::from_secs(3)).await;
            println!("async task finished");
            ()
        })
    });

    run_task(Task::Async(task), tx.inner().clone());
    "task started"
}

type AsyncTask = Box<dyn (FnOnce() -> Pin<Box<dyn Future<Output = ()>>>) + Send>;
type SyncTask = Box<dyn FnOnce() + Send>;

enum Task {
    Sync(SyncTask),
    Async(AsyncTask),
}

async fn task_exec(mut rx: Receiver<Task>) {
    println!("task executer started");
    // loop forever
    loop {
        if let Some(task) = rx.recv().await {
            println!("start new task");
            use Task::*;
            match task {
                // call the closure sync
                Sync(t) => t(),
                // call the closure async
                Async(t) => t().await,
            }
        }
    }
}

/// A simple single threaded runtime
fn simple_async_runtime() -> std::io::Result<Runtime> {
    Builder::new().enable_all().build()
}

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

fn main() {
    let (tx, rx) = mpsc::channel(32);

    // this thread will fetch tasks from the channels receiving end and execute them
    std::thread::spawn(|| {
        let mut rt = simple_async_runtime().unwrap();
        rt.block_on(task_exec(rx))
    });

    rocket::ignite()
        .manage(tx)
        // already added all endpoints
        .mount("/", routes!(index, sync_task_endpoint, async_task_endpoint))
        .launch();
}
